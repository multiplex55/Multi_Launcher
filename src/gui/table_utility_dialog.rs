use crate::clipboard_modify::clipboard::{
    ArboardClipboardBackend, ClipboardBackend, ClipboardError,
};
use crate::common::atomic_file;
use crate::table_utility::{
    DelimiterChoice, EffectiveSortMode, RowWidthPolicy, TableDelimiter, TableDocument,
    TableJsonShape, TableSort, TableSortDirection, TableSortMode, TableWarning,
};
use crate::table_utility_session::{
    TableExportKind, TableExportOperation, TableExportPayload, TableExportReceipt,
    TableSessionSnapshot, TableSourceLoadState, TableUtilitySession,
};
use eframe::egui;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RESULT_ROW_HEIGHT: f32 = 28.0;
const SOURCE_PREVIEW_CHARS: usize = 2_048;

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct TestWidgetResponse {
    id: egui::Id,
    rect: egui::Rect,
    enabled: bool,
}

#[derive(Clone)]
enum ExportAction {
    Clipboard,
    SaveJson(PathBuf),
}

#[derive(Clone)]
struct PendingExport {
    operation: TableExportOperation,
    action: ExportAction,
}

struct PendingOverwrite {
    target: PathBuf,
    receipt: TableExportReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveOutcome {
    Saved,
    OverwriteRequired,
}

pub struct TableUtilityDialogState {
    pub open: bool,
    session: TableUtilitySession,
    clipboard: Arc<dyn ClipboardBackend>,
    source_editor: String,
    source_editor_revision: Option<u64>,
    filter_editor: String,
    json_shape: TableJsonShape,
    include_headers: bool,
    pending_export: Option<PendingExport>,
    pending_overwrite: Option<PendingOverwrite>,
    combo_anchor_ids: [Option<egui::Id>; 3],
    status: Option<String>,
    session_open_error: Option<String>,
    #[cfg(test)]
    test_delimiter_option_ids: Vec<(DelimiterChoice, egui::Id)>,
    #[cfg(test)]
    test_first_header_column_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_first_data_column_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_results_horizontal_offset: f32,
    #[cfg(test)]
    test_results_scroll_area_id: Option<egui::Id>,
    #[cfg(test)]
    test_results_content_size: egui::Vec2,
    #[cfg(test)]
    test_results_inner_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_replace_response_id: Option<egui::Id>,
    #[cfg(test)]
    test_cancel_overwrite_response_id: Option<egui::Id>,
    #[cfg(test)]
    test_paste_response: Option<TestWidgetResponse>,
    #[cfg(test)]
    test_open_response: Option<TestWidgetResponse>,
    #[cfg(test)]
    test_copy_tsv_response: Option<TestWidgetResponse>,
    #[cfg(test)]
    test_copy_json_response: Option<TestWidgetResponse>,
    #[cfg(test)]
    test_save_json_response: Option<TestWidgetResponse>,
    #[cfg(test)]
    test_source_editor_response: Option<TestWidgetResponse>,
    #[cfg(test)]
    test_source_preview_chars: usize,
    #[cfg(test)]
    test_source_preview_truncated: bool,
    #[cfg(test)]
    test_rendered_row_range: Option<(usize, usize)>,
    #[cfg(test)]
    test_visible_row_count: usize,
    #[cfg(test)]
    test_open_file_path: Option<PathBuf>,
    #[cfg(test)]
    test_save_json_path: Option<PathBuf>,
}

impl Default for TableUtilityDialogState {
    fn default() -> Self {
        Self {
            open: false,
            session: TableUtilitySession::new(),
            clipboard: Arc::new(ArboardClipboardBackend),
            source_editor: String::new(),
            source_editor_revision: None,
            filter_editor: String::new(),
            json_shape: TableJsonShape::Auto,
            include_headers: false,
            pending_export: None,
            pending_overwrite: None,
            combo_anchor_ids: [None; 3],
            status: None,
            session_open_error: None,
            #[cfg(test)]
            test_delimiter_option_ids: Vec::new(),
            #[cfg(test)]
            test_first_header_column_rect: None,
            #[cfg(test)]
            test_first_data_column_rect: None,
            #[cfg(test)]
            test_results_horizontal_offset: 0.0,
            #[cfg(test)]
            test_results_scroll_area_id: None,
            #[cfg(test)]
            test_results_content_size: egui::Vec2::ZERO,
            #[cfg(test)]
            test_results_inner_rect: None,
            #[cfg(test)]
            test_replace_response_id: None,
            #[cfg(test)]
            test_cancel_overwrite_response_id: None,
            #[cfg(test)]
            test_paste_response: None,
            #[cfg(test)]
            test_open_response: None,
            #[cfg(test)]
            test_copy_tsv_response: None,
            #[cfg(test)]
            test_copy_json_response: None,
            #[cfg(test)]
            test_save_json_response: None,
            #[cfg(test)]
            test_source_editor_response: None,
            #[cfg(test)]
            test_source_preview_chars: 0,
            #[cfg(test)]
            test_source_preview_truncated: false,
            #[cfg(test)]
            test_rendered_row_range: None,
            #[cfg(test)]
            test_visible_row_count: 0,
            #[cfg(test)]
            test_open_file_path: None,
            #[cfg(test)]
            test_save_json_path: None,
        }
    }
}

impl TableUtilityDialogState {
    pub fn open(&mut self) {
        self.open = true;
        self.pending_overwrite = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.session.close();
        self.pending_export = None;
        self.pending_overwrite = None;
        self.status = None;
    }

    pub(crate) fn overwrite_confirmation_layer_is_current(&self, ctx: &egui::Context) -> bool {
        let Some(pending) = self.pending_overwrite.as_ref() else {
            return false;
        };
        if !self.open
            || egui::Popup::is_any_open(ctx)
            || ctx.memory(|memory| memory.top_modal_layer().is_some())
        {
            return false;
        }
        let layer = overwrite_confirmation_layer_id(&pending.receipt);
        let parent_layer = crate::gui::jump_mode::ui::NavigationDialogKind::TableUtility.layer_id();
        self.session.lifetime() == pending.receipt.lifetime
            && crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(layer)
            && crate::gui::jump_mode::ui::focused_response_layer(ctx).is_none_or(|focused| {
                focused == layer
                    || parent_layer == Some(focused)
                    || focused.order != egui::Order::Middle
            })
    }

    pub(crate) fn owned_combo_popup_is_current(&self, ctx: &egui::Context) -> bool {
        use crate::gui::jump_mode::ui::{NavigationDialogKind, focused_response_layer};

        if !self.open
            || !egui::Popup::is_any_open(ctx)
            || ctx.viewport_id() != egui::ViewportId::ROOT
        {
            return false;
        }
        let open_anchors = self
            .combo_anchor_ids
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let response = ctx.read_response(id)?;
                (response.layer_id == NavigationDialogKind::TableUtility.layer_id()?)
                    .then_some(response)
            })
            .filter(|response| egui::ComboBox::is_open(ctx, response.id))
            .collect::<Vec<_>>();
        let [anchor] = open_anchors.as_slice() else {
            return false;
        };
        let Some(parent_layer) = NavigationDialogKind::TableUtility.layer_id() else {
            return false;
        };
        let popup_id = egui::Popup::default_response_id(anchor);
        if !egui::Popup::is_id_open(ctx, popup_id) {
            return false;
        }
        let popup_layer = egui::LayerId::new(egui::Order::Foreground, popup_id);
        let Some(rect) = ctx.memory(|memory| memory.area_rect(popup_id)) else {
            return false;
        };
        let top_foreground = ctx.memory(|memory| {
            memory
                .layer_ids()
                .filter(|layer| {
                    layer.order == egui::Order::Foreground
                        && memory.areas().visible_last_frame(layer)
                })
                .last()
        });
        let popup_is_sizing = egui::containers::AreaState::load(ctx, popup_id)
            .is_some_and(|state| !state.interactable);
        let popup_is_current = ctx.layer_id_at(rect.center()) == Some(popup_layer);
        crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(parent_layer)
            && rect.is_positive()
            && top_foreground == Some(popup_layer)
            && (popup_is_current || popup_is_sizing)
            && focused_response_layer(ctx)
                .is_none_or(|focused| focused == parent_layer || focused == popup_layer)
    }

    #[cfg(test)]
    pub(crate) fn delimiter_option_response_id_for_test(
        &self,
        option: DelimiterChoice,
    ) -> Option<egui::Id> {
        self.test_delimiter_option_ids
            .iter()
            .find_map(|(candidate, id)| (*candidate == option).then_some(*id))
    }

    #[cfg(test)]
    pub(crate) fn overwrite_response_id_for_test(&self, replace: bool) -> Option<egui::Id> {
        if replace {
            self.test_replace_response_id
        } else {
            self.test_cancel_overwrite_response_id
        }
    }

    #[cfg(test)]
    pub(crate) fn test_results_layout(
        &self,
    ) -> (Option<egui::Id>, egui::Vec2, Option<egui::Rect>, f32) {
        (
            self.test_results_scroll_area_id,
            self.test_results_content_size,
            self.test_results_inner_rect,
            self.test_results_horizontal_offset,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_prepare_overwrite_confirmation(
        &mut self,
        ctx: &egui::Context,
        target: PathBuf,
    ) -> Result<(), String> {
        self.open();
        self.session.open(ctx)?;
        self.session.set_has_header(true);
        self.session
            .paste_source("name,value\nalpha,1\n".to_owned())?;
        let parsed = wait_for_test_snapshot(&mut self.session)?;
        let row = parsed
            .projection
            .as_ref()
            .and_then(|projection| projection.visible_rows().first())
            .copied()
            .ok_or_else(|| "The overwrite fixture has no visible row.".to_owned())?;
        self.session.set_selection(
            Some(std::collections::HashSet::from([row])),
            std::collections::HashSet::from([0]),
        );
        let operation = self
            .session
            .request_export(TableExportKind::Json(TableJsonShape::Arrays), false)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            self.session.poll();
            if let Some(receipt) = self.session.take_export_receipt(operation) {
                return self.save_receipt(target, receipt);
            }
            if std::time::Instant::now() >= deadline {
                return Err("The test export did not complete.".into());
            }
            std::thread::yield_now();
        }
    }

    pub(crate) fn cancel_overwrite_confirmation(&mut self) -> bool {
        if self.pending_overwrite.take().is_some() {
            self.status = Some("JSON replacement was cancelled.".into());
            true
        } else {
            false
        }
    }

    pub fn shutdown(&mut self) {
        self.close();
        self.session.shutdown();
    }

    #[cfg(test)]
    pub(crate) fn set_clipboard_backend_for_test(&mut self, clipboard: Arc<dyn ClipboardBackend>) {
        self.clipboard = clipboard;
    }

    pub(crate) fn show_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        owner: crate::gui::jump_mode::ui::NavigationOwner,
        application_input_allowed: bool,
        native_owner_current: bool,
        first_ui_pass: bool,
    ) {
        if !self.open {
            return;
        }
        #[cfg(test)]
        {
            self.test_paste_response = None;
            self.test_open_response = None;
            self.test_copy_tsv_response = None;
            self.test_copy_json_response = None;
            self.test_save_json_response = None;
            self.test_source_editor_response = None;
            self.test_source_preview_chars = 0;
            self.test_source_preview_truncated = false;
            self.test_rendered_row_range = None;
            self.test_visible_row_count = 0;
        }
        if let Err(error) = self.session.open(ctx) {
            self.session_open_error = Some(error);
        }
        self.session.poll();
        self.finish_export_if_ready();

        let snapshot = self.session.snapshot();
        self.sync_editors(&snapshot);
        let jump_scope = scope.zip(instance).and_then(|(scope, instance)| {
            matches!(
                owner,
                crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                    kind: crate::gui::jump_mode::ui::NavigationDialogKind::TableUtility,
                    instance: owner_instance,
                } if owner_instance == instance
            )
            .then_some(scope)
        });
        let generation = target_generation(&snapshot);
        let table_scope = jump_scope.zip(instance).map(|(scope, instance)| {
            scope.child_namespace("table-utility").with_surface(
                crate::gui::jump_mode::SurfaceId::dialog("table-utility", instance),
                crate::gui::jump_mode::TargetGeneration(generation),
            )
        });

        let parent_layer_current = crate::gui::jump_mode::ui::NavigationDialogKind::TableUtility
            .layer_id()
            .is_some_and(|layer| {
                crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(layer)
            });
        let native_input_owner = native_owner_current && application_input_allowed;
        let combo_popup_current = native_input_owner && self.owned_combo_popup_is_current(ctx);
        let main_owner_current =
            native_input_owner && parent_layer_current && !egui::Popup::is_any_open(ctx);
        let response_owner_current = main_owner_current || combo_popup_current;
        let mut window_open = self.open;
        let mut visual_order = 0_u64;
        let mut close_requested = false;
        let committing_owner = first_ui_pass && response_owner_current;
        egui::Window::new("Table Utility")
            .id(egui::Id::new(Some("Table Utility")))
            .collapsible(false)
            .resizable(true)
            .default_size((1180.0, 760.0))
            .min_size((600.0, 420.0))
            .open(&mut window_open)
            .show(ctx, |ui| {
                let visible_owner =
                    response_owner_current && ui.is_visible() && !ui.is_sizing_pass();
                let main_response_owner = visible_owner && main_owner_current;
                let combo_response_owner = visible_owner;
                let main_committing_owner = committing_owner && main_owner_current;
                let combo_committing_owner = committing_owner && combo_response_owner;
                ui.add_enabled_ui(visible_owner, |ui| {
                    if let Some(error) = &self.session_open_error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    let body_max_height = (ui.available_height() - 96.0).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("table-utility-body")
                        .max_height(body_max_height)
                        .show(ui, |ui| {
                            self.show_controls(
                                ui,
                                &snapshot,
                                main_response_owner,
                                main_committing_owner,
                                combo_response_owner,
                                combo_committing_owner,
                                table_scope.as_ref(),
                                &mut visual_order,
                            );
                            ui.separator();
                            ui.add_enabled_ui(main_response_owner, |ui| {
                                self.show_source(
                                    ui,
                                    &snapshot,
                                    main_response_owner,
                                    main_committing_owner,
                                    table_scope.as_ref(),
                                    &mut visual_order,
                                );
                                ui.separator();
                                self.show_results(
                                    ui,
                                    &snapshot,
                                    main_response_owner,
                                    main_committing_owner,
                                    table_scope.as_ref(),
                                    &mut visual_order,
                                );
                            });
                        });
                    ui.separator();
                    self.show_footer(
                        ui,
                        &snapshot,
                        main_response_owner,
                        main_committing_owner,
                        combo_response_owner,
                        combo_committing_owner,
                        table_scope.as_ref(),
                        &mut visual_order,
                        &mut close_requested,
                    );
                    if let Some(message) = snapshot.message.as_deref().or(self.status.as_deref()) {
                        ui.label(message);
                    }
                });
            });

        if first_ui_pass
            && native_owner_current
            && application_input_allowed
            && (!window_open || close_requested)
        {
            self.close();
        }
        self.show_overwrite_confirmation(ctx, native_input_owner, first_ui_pass);
    }

    fn sync_editors(&mut self, snapshot: &TableSessionSnapshot) {
        if self.source_editor_revision != Some(snapshot.source_display_revision) {
            self.source_editor.clear();
            if snapshot.source_is_editable() {
                self.source_editor.push_str(snapshot.source.as_ref());
            }
            self.source_editor_revision = Some(snapshot.source_display_revision);
        }
        if self.filter_editor != self.session.view_options().filter {
            self.filter_editor = self.session.view_options().filter.clone();
        }
    }

    fn show_controls(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TableSessionSnapshot,
        enabled: bool,
        commit_enabled: bool,
        combo_enabled: bool,
        combo_commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        ui.horizontal_wrapped(|ui| {
            let paste = ui.add_enabled(enabled, egui::Button::new("Paste Text"));
            #[cfg(test)]
            {
                self.test_paste_response = Some(TestWidgetResponse {
                    id: paste.id,
                    rect: paste.rect,
                    enabled: paste.enabled(),
                });
            }
            register_target(
                scope,
                ui,
                &paste,
                "action/paste-text",
                "Paste text from the clipboard",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if paste.clicked() && commit_enabled {
                match self.clipboard.read_text() {
                    Ok(text) => match self.session.paste_source(text) {
                        Ok(()) => self.status = Some("Clipboard text loaded; parsing…".into()),
                        Err(error) => self.status = Some(error),
                    },
                    Err(error) => self.status = Some(clipboard_message(&error).into()),
                }
            }

            let open_file = ui.add_enabled(enabled, egui::Button::new("Open File…"));
            #[cfg(test)]
            {
                self.test_open_response = Some(TestWidgetResponse {
                    id: open_file.id,
                    rect: open_file.rect,
                    enabled: open_file.enabled(),
                });
            }
            register_target(
                scope,
                ui,
                &open_file,
                "action/open-file",
                "Open a UTF-8 delimited text file",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if open_file.clicked() && commit_enabled {
                #[cfg(test)]
                let selected_path = self.test_open_file_path.take();
                #[cfg(not(test))]
                let selected_path = rfd::FileDialog::new()
                    .add_filter("Delimited text", &["csv", "tsv", "txt"])
                    .pick_file();
                if let Some(path) = selected_path {
                    match self.session.request_file_read(path) {
                        Ok(()) => self.status = None,
                        Err(error) => self.status = Some(error),
                    }
                }
            }

            ui.separator();
            let mut delimiter = snapshot.parse_options.delimiter;
            #[cfg(test)]
            self.test_delimiter_option_ids.clear();
            let delimiter_response = ui.add_enabled_ui(combo_enabled, |ui| {
                egui::ComboBox::from_id_salt("table-delimiter")
                    .selected_text(match delimiter {
                        DelimiterChoice::Auto => "Auto delimiter",
                        DelimiterChoice::Explicit(TableDelimiter::Comma) => "CSV comma",
                        DelimiterChoice::Explicit(TableDelimiter::Tab) => "TSV tab",
                    })
                    .show_ui(ui, |ui| {
                        let _auto =
                            ui.selectable_value(&mut delimiter, DelimiterChoice::Auto, "Auto");
                        #[cfg(test)]
                        self.test_delimiter_option_ids
                            .push((DelimiterChoice::Auto, _auto.id));
                        let _comma = ui.selectable_value(
                            &mut delimiter,
                            DelimiterChoice::Explicit(TableDelimiter::Comma),
                            "CSV (comma)",
                        );
                        #[cfg(test)]
                        self.test_delimiter_option_ids
                            .push((DelimiterChoice::Explicit(TableDelimiter::Comma), _comma.id));
                        let _tab = ui.selectable_value(
                            &mut delimiter,
                            DelimiterChoice::Explicit(TableDelimiter::Tab),
                            "TSV (tab)",
                        );
                        #[cfg(test)]
                        self.test_delimiter_option_ids
                            .push((DelimiterChoice::Explicit(TableDelimiter::Tab), _tab.id));
                    })
            });
            self.combo_anchor_ids[0] = Some(delimiter_response.inner.response.id);
            register_target(
                scope,
                ui,
                &delimiter_response.inner.response,
                "parse/delimiter",
                "Choose Auto, CSV, or TSV delimiter handling",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if combo_commit_enabled && delimiter != snapshot.parse_options.delimiter {
                self.session.set_delimiter(delimiter);
            }

            let mut has_header = snapshot.parse_options.has_header;
            let has_header_response =
                ui.add_enabled(enabled, egui::Checkbox::new(&mut has_header, "Has Header"));
            register_target(
                scope,
                ui,
                &has_header_response,
                "parse/has-header",
                "Treat the first record as column headers",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if has_header_response.changed() && commit_enabled {
                self.session.set_has_header(has_header);
            }

            let mut width_policy = snapshot.parse_options.row_width;
            let width_response = ui.add_enabled_ui(combo_enabled, |ui| {
                egui::ComboBox::from_id_salt("table-row-width")
                    .selected_text(match width_policy {
                        RowWidthPolicy::Lenient => "Lenient row widths",
                        RowWidthPolicy::Strict => "Strict row widths",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut width_policy, RowWidthPolicy::Lenient, "Lenient");
                        ui.selectable_value(&mut width_policy, RowWidthPolicy::Strict, "Strict");
                    })
            });
            self.combo_anchor_ids[1] = Some(width_response.inner.response.id);
            register_target(
                scope,
                ui,
                &width_response.inner.response,
                "parse/row-width",
                "Choose lenient or strict row width handling",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if combo_commit_enabled && width_policy != snapshot.parse_options.row_width {
                self.session
                    .set_parse_options(crate::table_utility::TableParseOptions {
                        row_width: width_policy,
                        ..snapshot.parse_options
                    });
            }

            let parse = ui.add_enabled(enabled, egui::Button::new("Parse / Refresh"));
            register_target(
                scope,
                ui,
                &parse,
                "action/parse-refresh",
                "Parse or refresh the current source",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if parse.clicked() && commit_enabled {
                self.session.request_parse();
                self.status = Some("Parsing the current source…".into());
            }
        });
        if matches!(
            &snapshot.source_load_state,
            TableSourceLoadState::Reading(_)
        ) {
            ui.label("Reading the selected file on the table worker…");
        }
        if !snapshot.accepted_options_are_current() && snapshot.document.is_some() {
            ui.weak("The results below are the last successful parse, with its recorded delimiter and options.");
        }
        if let Some(document) = &snapshot.document {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "Loaded: {} rows × {} columns · {}",
                    document.rows().len(),
                    document.column_count(),
                    match document.delimiter() {
                        TableDelimiter::Comma => "comma-delimited",
                        TableDelimiter::Tab => "tab-delimited",
                    }
                ));
                ui.weak(format!(
                    "{}header · {} widths",
                    if document.options().has_header {
                        "Has "
                    } else {
                        "No "
                    },
                    match document.options().row_width {
                        RowWidthPolicy::Lenient => "lenient",
                        RowWidthPolicy::Strict => "strict",
                    }
                ));
                if let Some(effective) = snapshot
                    .projection
                    .as_ref()
                    .and_then(|projection| projection.effective_sort_mode())
                {
                    ui.weak(match effective {
                        EffectiveSortMode::Numeric => "AutoNumeric sort: numeric",
                        EffectiveSortMode::Lexical => "Sort: lexical",
                    });
                }
            });
            show_warnings(ui, document);
        }
    }

    fn show_source(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TableSessionSnapshot,
        enabled: bool,
        commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        ui.label("Source (UTF-8; parsing is explicit)");
        if snapshot.source_is_editable() {
            let mut source_editor = self.source_editor.clone();
            let response = ui.add_enabled(
                enabled && !snapshot.reading_source,
                egui::TextEdit::multiline(&mut source_editor)
                    .desired_rows(5)
                    .lock_focus(true),
            );
            #[cfg(test)]
            {
                self.test_source_editor_response = Some(TestWidgetResponse {
                    id: response.id,
                    rect: response.rect,
                    enabled: response.enabled(),
                });
            }
            register_target(
                scope,
                ui,
                &response,
                "source/text",
                "Delimited text source",
                crate::gui::jump_mode::TargetKind::TextEntry,
                order,
            );
            if response.changed() && commit_enabled {
                match self.session.inline_source_changed(source_editor.clone()) {
                    Ok(()) => {
                        self.source_editor = source_editor;
                        self.status = None;
                    }
                    Err(error) => self.status = Some(error),
                }
            }
        } else {
            ui.label(format!(
                "{} bytes retained; inline editing is limited to {} bytes and {} lines. Use Paste Text or Open File to replace it.",
                snapshot.source.len(),
                crate::table_utility_session::TABLE_INLINE_EDIT_MAX_BYTES,
                crate::table_utility_session::TABLE_INLINE_EDIT_MAX_LINES,
            ));
            let preview = bounded_text(&snapshot.source, SOURCE_PREVIEW_CHARS);
            #[cfg(test)]
            {
                self.test_source_preview_chars = preview.chars().count();
                self.test_source_preview_truncated = preview.ends_with('…');
            }
            egui::ScrollArea::vertical()
                .id_salt("table-large-source-preview")
                .max_height(112.0)
                .show(ui, |ui| {
                    ui.monospace(preview);
                });
        }
    }

    fn show_results(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TableSessionSnapshot,
        enabled: bool,
        commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        ui.horizontal(|ui| {
            ui.label("Filter visible rows:");
            let mut filter_editor = self.filter_editor.clone();
            let response = ui.add_enabled(
                enabled,
                egui::TextEdit::singleline(&mut filter_editor)
                    .desired_width(240.0)
                    .hint_text("case-insensitive literal search"),
            );
            register_target(
                scope,
                ui,
                &response,
                "view/filter",
                "Filter table rows",
                crate::gui::jump_mode::TargetKind::TextEntry,
                order,
            );
            if response.changed() && commit_enabled {
                self.filter_editor = filter_editor;
                self.session.set_filter(self.filter_editor.clone());
            }
            let sort_mode = snapshot
                .projection
                .as_ref()
                .and_then(|projection| projection.effective_sort_mode());
            ui.weak(match sort_mode {
                Some(EffectiveSortMode::Numeric) => "AutoNumeric: numeric values",
                Some(EffectiveSortMode::Lexical) => "Sort values lexically",
                None => "Unsorted",
            });
        });
        let Some(document) = snapshot.document.as_deref() else {
            ui.weak("No parsed table is available yet.");
            return;
        };
        let Some(projection) = snapshot.projection.as_deref() else {
            ui.weak("Building the current result view…");
            return;
        };
        if !matches!(
            &snapshot.projection_state,
            crate::table_utility_session::TableProjectionState::Ready(_)
        ) {
            ui.weak(
                "The rows below are the last completed view; the current projection is updating.",
            );
        }
        let visible = projection.visible_rows();
        #[cfg(test)]
        {
            self.test_visible_row_count = visible.len();
        }
        let selected_count = snapshot.selected_visible_row_count;
        ui.label(format!(
            "{} visible rows · {} selected · {} columns available",
            visible.len(),
            selected_count,
            document.column_count()
        ));

        let mut sort = self.session.view_options().sort;
        let header_height = 32.0;
        #[cfg(test)]
        {
            self.test_first_header_column_rect = None;
            self.test_first_data_column_rect = None;
            self.test_results_horizontal_offset = 0.0;
        }
        let results = egui::ScrollArea::horizontal()
            .id_salt("table-results-horizontal")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([30.0, header_height], egui::Label::new("Row").wrap());
                    for column in 0..document.column_count() {
                        let label = header_label(document, column);
                        let sort_mark = sort.map_or("", |current| {
                            if current.column != column {
                                ""
                            } else if current.direction == TableSortDirection::Ascending {
                                " ↑"
                            } else {
                                " ↓"
                            }
                        });
                        let _slot = ui.allocate_ui_with_layout(
                            egui::vec2(180.0, header_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |cell_ui| {
                                let sort_response = cell_ui.add_sized(
                                    [148.0, header_height],
                                    egui::Button::new(format!("{label}{sort_mark}")),
                                );
                                register_target(
                                    scope,
                                    cell_ui,
                                    &sort_response,
                                    format!("column/{column}/sort"),
                                    format!("Sort by {label}"),
                                    crate::gui::jump_mode::TargetKind::Button,
                                    order,
                                );
                                if sort_response.clicked() && commit_enabled {
                                    sort = Some(TableSort {
                                        column,
                                        direction: if sort.is_some_and(|current| {
                                            current.column == column
                                                && current.direction
                                                    == TableSortDirection::Ascending
                                        }) {
                                            TableSortDirection::Descending
                                        } else {
                                            TableSortDirection::Ascending
                                        },
                                        mode: TableSortMode::AutoNumeric,
                                    });
                                    self.session.set_sort(sort);
                                }
                                let mut selected = snapshot.selected_columns.contains(&column);
                                let column_response = cell_ui.add_sized(
                                    [24.0, header_height],
                                    egui::Checkbox::new(&mut selected, ""),
                                );
                                register_target(
                                    scope,
                                    cell_ui,
                                    &column_response,
                                    format!("column/{column}/select"),
                                    format!("Include {label} in selected-column output"),
                                    crate::gui::jump_mode::TargetKind::Button,
                                    order,
                                );
                                if column_response.changed() && commit_enabled {
                                    self.session.set_column_selected(column, selected);
                                }
                            },
                        );
                        #[cfg(test)]
                        if column == 0 {
                            self.test_first_header_column_rect = Some(_slot.response.rect);
                        }
                    }
                });
                let selected_rows = snapshot.selected_rows.as_ref();
                let active_projection = snapshot.projection.as_ref();
                if let Some(active_projection) = active_projection {
                    egui::ScrollArea::vertical()
                        .id_salt("table-results-rows")
                        .max_height(250.0)
                        .show_rows(ui, RESULT_ROW_HEIGHT, visible.len(), |ui, range| {
                            #[cfg(test)]
                            {
                                self.test_rendered_row_range = Some((range.start, range.end));
                            }
                            for visible_index in range {
                                let Some(row_id) =
                                    active_projection.visible_rows().get(visible_index).copied()
                                else {
                                    continue;
                                };
                                let Some(row) = document.row(row_id) else {
                                    continue;
                                };
                                ui.horizontal(|ui| {
                                    let mut selected =
                                        selected_rows.is_none_or(|rows| rows.contains(&row_id));
                                    let row_response = ui.add_sized(
                                        [30.0, RESULT_ROW_HEIGHT],
                                        egui::Checkbox::new(&mut selected, ""),
                                    );
                                    register_target(
                                        scope,
                                        ui,
                                        &row_response,
                                        format!("row/{}/select", row_id.source_index()),
                                        format!("Select displayed row {}", visible_index + 1),
                                        crate::gui::jump_mode::TargetKind::Button,
                                        order,
                                    );
                                    if row_response.changed() && commit_enabled {
                                        self.session.set_row_selected(row_id, selected);
                                    }
                                    for column in 0..document.column_count() {
                                        let text = document
                                            .bounded_preview(row_id, column)
                                            .unwrap_or_default();
                                        let _cell_response = ui.add_sized(
                                            [180.0, RESULT_ROW_HEIGHT],
                                            egui::Label::new(text)
                                                .truncate()
                                                .sense(egui::Sense::hover()),
                                        );
                                        #[cfg(test)]
                                        if visible_index == 0 && column == 0 {
                                            self.test_first_data_column_rect =
                                                Some(_cell_response.rect);
                                        }
                                    }
                                    let _ = row;
                                });
                            }
                        });
                }
            });
        #[cfg(test)]
        {
            self.test_results_horizontal_offset = results.state.offset.x;
            self.test_results_scroll_area_id = Some(results.id);
            self.test_results_content_size = results.content_size;
            self.test_results_inner_rect = Some(results.inner_rect);
        }
        let _ = results;
    }

    fn show_footer(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TableSessionSnapshot,
        enabled: bool,
        commit_enabled: bool,
        combo_enabled: bool,
        combo_commit_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
        close_requested: &mut bool,
    ) {
        let ready = enabled && snapshot.can_export() && snapshot.selected_visible_row_count > 0;
        ui.horizontal_wrapped(|ui| {
            let mut include_headers_choice = self.include_headers;
            let include_headers = ui.add_enabled(
                enabled,
                egui::Checkbox::new(&mut include_headers_choice, "Include TSV headers"),
            );
            if include_headers.changed() && commit_enabled {
                self.include_headers = include_headers_choice;
            }
            register_target(
                scope,
                ui,
                &include_headers,
                "output/include-headers",
                "Include selected column headers in TSV output",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            let tsv = ui.add_enabled(ready, egui::Button::new("Copy Selected Columns"));
            #[cfg(test)]
            {
                self.test_copy_tsv_response = Some(TestWidgetResponse {
                    id: tsv.id,
                    rect: tsv.rect,
                    enabled: tsv.enabled(),
                });
            }
            register_target(
                scope,
                ui,
                &tsv,
                "output/copy-tsv",
                "Copy the current visible row and column selection as TSV",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if tsv.clicked() && commit_enabled {
                self.queue_export(TableExportKind::Tsv, ExportAction::Clipboard);
            }

            let mut json_shape_choice = self.json_shape;
            let json_shape = ui.add_enabled_ui(combo_enabled, |ui| {
                egui::ComboBox::from_id_salt("table-json-shape")
                    .selected_text(match json_shape_choice {
                        TableJsonShape::Auto => "JSON Auto",
                        TableJsonShape::Objects => "JSON Objects",
                        TableJsonShape::Arrays => "JSON Arrays",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut json_shape_choice, TableJsonShape::Auto, "Auto");
                        ui.selectable_value(
                            &mut json_shape_choice,
                            TableJsonShape::Objects,
                            "Objects",
                        );
                        ui.selectable_value(
                            &mut json_shape_choice,
                            TableJsonShape::Arrays,
                            "Arrays",
                        );
                    })
            });
            self.combo_anchor_ids[2] = Some(json_shape.inner.response.id);
            if combo_commit_enabled {
                self.json_shape = json_shape_choice;
            }
            register_target(
                scope,
                ui,
                &json_shape.inner.response,
                "output/json-shape",
                "Choose automatic, object, or array JSON output",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            let copy_json = ui.add_enabled(ready, egui::Button::new("Copy JSON"));
            register_target(
                scope,
                ui,
                &copy_json,
                "output/copy-json",
                "Copy the current selection as JSON",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if copy_json.clicked() && commit_enabled {
                self.queue_export(
                    TableExportKind::Json(self.json_shape),
                    ExportAction::Clipboard,
                );
            }
            #[cfg(test)]
            {
                self.test_copy_json_response = Some(TestWidgetResponse {
                    id: copy_json.id,
                    rect: copy_json.rect,
                    enabled: copy_json.enabled(),
                });
            }

            let save_json = ui.add_enabled(ready, egui::Button::new("Save JSON As…"));
            #[cfg(test)]
            {
                self.test_save_json_response = Some(TestWidgetResponse {
                    id: save_json.id,
                    rect: save_json.rect,
                    enabled: save_json.enabled(),
                });
            }
            register_target(
                scope,
                ui,
                &save_json,
                "output/save-json",
                "Save the current selection as JSON",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if save_json.clicked() && commit_enabled {
                #[cfg(test)]
                let selected_path = self.test_save_json_path.take();
                #[cfg(not(test))]
                let selected_path = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .save_file();
                if let Some(path) = selected_path {
                    if ensure_destination_is_not_source(snapshot.source_path.as_deref(), &path)
                        .is_ok()
                    {
                        self.queue_export(
                            TableExportKind::Json(self.json_shape),
                            ExportAction::SaveJson(path),
                        );
                    } else {
                        self.status = Some(
                            "Choose a different destination; the opened source file is read-only."
                                .into(),
                        );
                    }
                }
            }

            let close = ui.add_enabled(enabled, egui::Button::new("Close"));
            register_target(
                scope,
                ui,
                &close,
                "action/close",
                "Close Table Utility",
                crate::gui::jump_mode::TargetKind::Button,
                order,
            );
            if close.clicked() && commit_enabled {
                *close_requested = true;
            }
        });
        if self.json_shape == TableJsonShape::Auto
            && let Some(TableExportPayload::Json(json)) = self
                .pending_overwrite
                .as_ref()
                .map(|pending| &pending.receipt.payload)
            && let Some(reason) = json.fallback_reason
        {
            ui.weak(format!(
                "JSON Auto used arrays: {}.",
                fallback_reason_text(reason)
            ));
        }
    }

    fn queue_export(&mut self, kind: TableExportKind, action: ExportAction) {
        self.status = None;
        match self.session.request_export(kind, self.include_headers) {
            Ok(operation) => {
                self.pending_export = Some(PendingExport { operation, action });
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn finish_export_if_ready(&mut self) {
        let Some(pending) = self.pending_export.clone() else {
            return;
        };
        if let Some(receipt) = self.session.take_export_receipt(pending.operation) {
            self.pending_export = None;
            if !self.session.export_receipt_is_current(&receipt) {
                self.status =
                    Some("The selected table changed before the output could be used.".into());
                return;
            }
            match pending.action {
                ExportAction::Clipboard => {
                    let Some(payload) = payload_text(&receipt.payload) else {
                        self.status = Some("The requested output was not valid text.".into());
                        return;
                    };
                    match self.clipboard.write_text(payload) {
                        Ok(()) => {
                            self.status = Some("Copied the current table selection.".into());
                            if let TableExportPayload::Json(json) = receipt.payload
                                && let Some(reason) = json.fallback_reason
                            {
                                self.status = Some(format!(
                                    "Copied JSON arrays because {}.",
                                    fallback_reason_text(reason)
                                ));
                            }
                        }
                        Err(error) => self.status = Some(clipboard_message(&error).into()),
                    }
                }
                ExportAction::SaveJson(path) => {
                    if let Err(error) = self.save_receipt(path, receipt) {
                        self.status = Some(error);
                    }
                }
            }
        } else if self.session.snapshot().export_pending != Some(pending.operation) {
            self.pending_export = None;
        }
    }

    fn save_receipt(&mut self, path: PathBuf, receipt: TableExportReceipt) -> Result<(), String> {
        if !self.session.export_receipt_is_current(&receipt) {
            return Err("The selected table changed before it could be saved.".into());
        }
        let TableExportPayload::Json(json) = &receipt.payload else {
            return Err("Only JSON output can be saved from this action.".into());
        };
        let snapshot = self.session.snapshot();
        let target = resolve_target(&path)?;
        match save_export_target(
            snapshot.source_path.as_deref(),
            &target,
            json.text.as_bytes(),
            false,
        )? {
            SaveOutcome::Saved => {
                self.status = Some("Saved the selected table as JSON.".into());
            }
            SaveOutcome::OverwriteRequired => {
                self.pending_overwrite = Some(PendingOverwrite { target, receipt });
                self.status = Some("The file exists. Confirm replacement to continue.".into());
            }
        }
        Ok(())
    }

    fn show_overwrite_confirmation(
        &mut self,
        ctx: &egui::Context,
        owner_current: bool,
        first_ui_pass: bool,
    ) {
        #[cfg(test)]
        {
            self.test_replace_response_id = None;
            self.test_cancel_overwrite_response_id = None;
        }
        let Some(pending) = self.pending_overwrite.as_ref() else {
            return;
        };
        let confirmation_current =
            owner_current && self.overwrite_confirmation_layer_is_current(ctx);
        let replace_enabled =
            confirmation_current && self.session.export_receipt_is_current(&pending.receipt);
        let mut open = true;
        let mut decision = None;
        #[cfg(test)]
        let mut response_ids = None;
        egui::Window::new("Replace JSON file?")
            .id(overwrite_confirmation_window_id(&pending.receipt))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("The destination already exists. Replace it with this captured export?");
                ui.horizontal(|ui| {
                    let replace = ui.add_enabled(replace_enabled, egui::Button::new("Replace"));
                    let cancel = ui.add_enabled(confirmation_current, egui::Button::new("Cancel"));
                    #[cfg(test)]
                    {
                        response_ids = Some((replace.id, cancel.id));
                    }
                    if replace.clicked() && first_ui_pass {
                        decision = Some(true);
                    }
                    if cancel.clicked() && first_ui_pass {
                        decision = Some(false);
                    }
                });
            });
        #[cfg(test)]
        if let Some((replace, cancel)) = response_ids {
            self.test_replace_response_id = Some(replace);
            self.test_cancel_overwrite_response_id = Some(cancel);
        }
        if first_ui_pass && !open {
            decision = Some(false);
        }
        if !first_ui_pass {
            return;
        }
        let Some(confirm) = decision else {
            return;
        };
        let Some(pending) = self.pending_overwrite.take() else {
            return;
        };
        if !confirm {
            self.status = Some("JSON replacement was cancelled.".into());
            return;
        }
        if !confirmation_current || !self.session.export_receipt_is_current(&pending.receipt) {
            self.status = Some("The table changed; the captured JSON export was discarded.".into());
            return;
        }
        let TableExportPayload::Json(json) = &pending.receipt.payload else {
            self.status = Some("The captured output was no longer JSON.".into());
            return;
        };
        let snapshot = self.session.snapshot();
        match save_export_target(
            snapshot.source_path.as_deref(),
            &pending.target,
            json.text.as_bytes(),
            true,
        ) {
            Ok(SaveOutcome::Saved) => self.status = Some("Replaced the JSON destination.".into()),
            Ok(SaveOutcome::OverwriteRequired) => {
                self.status = Some("The destination changed; choose Save JSON As again.".into())
            }
            Err(error) => self.status = Some(error),
        }
    }
}

fn overwrite_confirmation_window_id(receipt: &TableExportReceipt) -> egui::Id {
    egui::Id::new((
        "table-json-overwrite-confirmation",
        receipt.lifetime.value(),
        receipt.operation.value(),
    ))
}

fn overwrite_confirmation_layer_id(receipt: &TableExportReceipt) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        overwrite_confirmation_window_id(receipt),
    )
}

#[cfg(test)]
fn wait_for_test_snapshot(
    session: &mut TableUtilitySession,
) -> Result<TableSessionSnapshot, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        session.poll();
        let snapshot = session.snapshot();
        if matches!(
            &snapshot.parse_state,
            crate::table_utility_session::TableParseState::Ready(_)
        ) && matches!(
            &snapshot.projection_state,
            crate::table_utility_session::TableProjectionState::Ready(_)
        ) {
            return Ok(snapshot);
        }
        if std::time::Instant::now() >= deadline {
            return Err("The test table did not finish parsing and projection.".into());
        }
        std::thread::yield_now();
    }
}

fn register_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    id: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    order: &mut u64,
) {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope, ui, response, id, label, kind, order,
    );
}

fn target_generation(snapshot: &TableSessionSnapshot) -> u64 {
    snapshot.source_revision.value().rotate_left(17)
        ^ snapshot.view_revision.value()
        ^ snapshot.lifetime.value()
}

fn header_label(document: &TableDocument, column: usize) -> String {
    document
        .headers()
        .and_then(|headers| headers.get(column))
        .and_then(Option::as_deref)
        .filter(|header| !header.is_empty())
        .map(|header| crate::table_utility::bounded_cell_preview(header, 64))
        .unwrap_or_else(|| format!("Column {}", column + 1))
}

fn show_warnings(ui: &mut egui::Ui, document: &TableDocument) {
    if document.diagnostics().is_empty() && document.omitted_diagnostic_count() == 0 {
        return;
    }
    egui::CollapsingHeader::new(format!(
        "Parse diagnostics ({}{} )",
        document.diagnostics().len(),
        if document.omitted_diagnostic_count() == 0 {
            String::new()
        } else {
            format!(" + {} omitted", document.omitted_diagnostic_count())
        }
    ))
    .show(ui, |ui| {
        for warning in document.diagnostics() {
            ui.label(warning_text(warning));
        }
        if document.omitted_diagnostic_count() > 0 {
            ui.label(format!(
                "{} additional diagnostics were omitted.",
                document.omitted_diagnostic_count()
            ));
        }
    });
}

fn warning_text(warning: &TableWarning) -> String {
    match warning {
        TableWarning::ShortRowPadded {
            record,
            actual_columns,
            padded_columns,
        } => {
            format!("Record {record} had {actual_columns} fields; padded {padded_columns} fields.")
        }
        TableWarning::SchemaWidened {
            record,
            previous_columns,
            new_columns,
        } => format!(
            "Record {record} widened the table from {previous_columns} to {new_columns} columns."
        ),
        TableWarning::HeaderLabelsMissing {
            first_column,
            count,
        } => format!(
            "{} header label(s) are missing, starting at column {}.",
            count,
            first_column + 1
        ),
        TableWarning::EmptyHeader { column } => {
            format!("Header for column {} is empty.", column + 1)
        }
        TableWarning::DuplicateHeader {
            column,
            first_column,
        } => format!(
            "Header for column {} duplicates column {}.",
            column + 1,
            first_column + 1
        ),
        TableWarning::AdditionalWarningsOmitted { count } => {
            format!("{count} additional diagnostics were omitted.")
        }
    }
}

fn bounded_text(text: &str, max_chars: usize) -> String {
    crate::table_utility::bounded_cell_preview(text, max_chars)
}

fn payload_text(payload: &TableExportPayload) -> Option<&str> {
    match payload {
        TableExportPayload::Tsv(text) => Some(text),
        TableExportPayload::Delimited { text, .. } => Some(text),
        TableExportPayload::Json(json) => Some(json.text.as_str()),
    }
}

fn clipboard_message(error: &ClipboardError) -> &'static str {
    match error {
        ClipboardError::NonText => "The clipboard does not contain text.",
        ClipboardError::Busy(_) | ClipboardError::Transient(_) => {
            "The clipboard is busy. Try the action again."
        }
        ClipboardError::InvalidContent(_) => "The clipboard text could not be read.",
        _ => "The clipboard operation failed.",
    }
}

fn fallback_reason_text(reason: crate::table_utility::JsonShapeFallbackReason) -> &'static str {
    use crate::table_utility::JsonShapeFallbackReason;
    match reason {
        JsonShapeFallbackReason::HeadersDisabled => "headers are disabled",
        JsonShapeFallbackReason::HeaderMissing => "one or more headers are missing",
        JsonShapeFallbackReason::EmptyHeader => "a selected header is empty",
        JsonShapeFallbackReason::DuplicateHeader => "selected headers are not unique",
    }
}

fn resolve_target(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Choose a destination folder.".to_owned())?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Could not resolve the destination folder: {error}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "Choose a destination file name.".to_owned())?;
    let target = parent.join(name);
    if target.exists() {
        fs::canonicalize(&target)
            .map_err(|error| format!("Could not resolve the destination file: {error}"))
    } else {
        Ok(target)
    }
}

fn same_resolved_path(source: Option<&Path>, target: &Path) -> bool {
    let Some(source) = source else {
        return false;
    };
    let target = resolve_target(target).unwrap_or_else(|_| target.to_path_buf());
    #[cfg(windows)]
    {
        source
            .to_string_lossy()
            .eq_ignore_ascii_case(&target.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        source == target
    }
}

fn ensure_destination_is_not_source(source: Option<&Path>, target: &Path) -> Result<(), String> {
    if same_resolved_path(source, target) {
        Err("Choose a different destination; the opened source file is read-only.".into())
    } else {
        Ok(())
    }
}

fn save_export_target(
    source: Option<&Path>,
    target: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
) -> Result<SaveOutcome, String> {
    ensure_destination_is_not_source(source, target)?;
    write_json_target(target, payload, overwrite_confirmed)
}

fn write_json_target(
    target: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
) -> Result<SaveOutcome, String> {
    let target = resolve_target(target)?;
    let exists = match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => return Err("The destination must be a regular file.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("Could not inspect the destination: {error}")),
    };
    if exists && !overwrite_confirmed {
        return Ok(SaveOutcome::OverwriteRequired);
    }
    if exists {
        atomic_file::save_atomic(&target, payload)
            .map_err(|error| format!("Could not replace the JSON file: {error:#}"))?;
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| "Choose a destination folder.".to_owned())?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| format!("Could not prepare the JSON file: {error}"))?;
        temporary
            .write_all(payload)
            .and_then(|()| temporary.flush())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| format!("Could not write the JSON file: {error}"))?;
        temporary.persist_noclobber(&target).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                "The destination appeared while saving; choose Save JSON As again.".to_owned()
            } else {
                format!("Could not create the JSON file: {}", error.error)
            }
        })?;
    }
    Ok(SaveOutcome::Saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_utility_session::{TableParseState, TableProjectionState};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn table_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 760.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn table_frame(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        events: Vec<egui::Event>,
    ) {
        let _ = table_frame_with_response(ctx, dialog, events, None);
    }

    fn table_frame_with_response(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        events: Vec<egui::Event>,
        response_id: Option<egui::Id>,
    ) -> Option<egui::Response> {
        let mut response = None;
        let output = ctx.run_ui(table_input(events), |ctx| {
            dialog.show_with_jump_mode(
                ctx,
                None,
                None,
                crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                true,
                true,
                true,
            );
            response = response_id.and_then(|id| ctx.read_response(id));
        });
        output.drop_without_applying_deltas();
        response
    }

    fn table_frame_with_combo_observation(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        anchor_id: egui::Id,
        option_id: egui::Id,
    ) -> (bool, Option<egui::Response>, Option<egui::Response>) {
        let mut observation = (false, None, None);
        let output = ctx.run_ui(table_input(Vec::new()), |ctx| {
            dialog.show_with_jump_mode(
                ctx,
                None,
                None,
                crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                true,
                true,
                true,
            );
            observation = (
                dialog.owned_combo_popup_is_current(ctx),
                ctx.read_response(anchor_id),
                ctx.read_response(option_id),
            );
        });
        output.drop_without_applying_deltas();
        observation
    }

    fn click_response(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        id: egui::Id,
    ) -> egui::Response {
        let response = table_frame_with_response(ctx, dialog, Vec::new(), Some(id))
            .expect("the real UI response was rendered");
        assert!(response.enabled(), "the real UI response is enabled");
        let position = response.rect.center();
        table_frame(
            ctx,
            dialog,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        table_frame(
            ctx,
            dialog,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        response
    }

    fn click_test_widget(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        response: impl Fn(&TableUtilityDialogState) -> Option<TestWidgetResponse>,
    ) -> TestWidgetResponse {
        table_frame(ctx, dialog, Vec::new());
        let initial = response(dialog).expect("the actual widget response is rendered");
        assert!(initial.enabled, "the actual widget is enabled");
        let initial_pos = initial.rect.center();

        table_frame(ctx, dialog, vec![egui::Event::PointerMoved(initial_pos)]);
        let moved = response(dialog).expect("the widget remains rendered after pointer move");
        assert_eq!(
            moved.id, initial.id,
            "the clicked response keeps its widget identity"
        );
        assert!(moved.enabled, "the actual widget remains enabled");
        let press_pos = moved.rect.center();
        table_frame(
            ctx,
            dialog,
            vec![egui::Event::PointerButton {
                pos: press_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        table_frame(
            ctx,
            dialog,
            vec![egui::Event::PointerButton {
                pos: press_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        moved
    }

    fn await_dialog(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        mut ready: impl FnMut(&TableSessionSnapshot) -> bool,
    ) -> TableSessionSnapshot {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            table_frame(ctx, dialog, Vec::new());
            let snapshot = dialog.session.snapshot();
            if ready(&snapshot) {
                return snapshot;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "table dialog did not reach the expected state: parse={:?}, projection={:?}, source={:?}",
                snapshot.parse_state,
                snapshot.projection_state,
                snapshot.source_load_state
            );
            std::thread::yield_now();
        }
    }

    fn prepare_dialog_table(
        ctx: &egui::Context,
        dialog: &mut TableUtilityDialogState,
        source: &str,
    ) {
        dialog.open();
        dialog.session.open(ctx).expect("start table worker");
        dialog.session.set_has_header(true);
        dialog
            .session
            .paste_source(source.to_owned())
            .expect("queue table source");
        wait_for_test_snapshot(&mut dialog.session).expect("table becomes ready");
    }

    #[derive(Clone, Default)]
    struct ClipboardProbe {
        reads: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
        read_value: Arc<std::sync::Mutex<String>>,
        written_values: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl ClipboardBackend for ClipboardProbe {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .read_value
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone())
        }

        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.written_values
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(text.to_owned());
            Ok(())
        }
    }

    fn clipboard_writes(probe: &ClipboardProbe) -> Vec<String> {
        probe
            .written_values
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    #[test]
    fn table_dialog_open_and_close_do_not_read_clipboard_implicitly() {
        let context = egui::Context::default();
        let probe = ClipboardProbe::default();
        let mut dialog = TableUtilityDialogState::default();
        dialog.set_clipboard_backend_for_test(Arc::new(probe.clone()));
        dialog.open();
        let output = context.run_ui(egui::RawInput::default(), |ctx| {
            dialog.show_with_jump_mode(
                ctx,
                None,
                None,
                crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                false,
                false,
                true,
            );
        });
        output.drop_without_applying_deltas();
        dialog.close();
        assert_eq!(probe.reads.load(Ordering::SeqCst), 0);
        assert_eq!(probe.writes.load(Ordering::SeqCst), 0);
        dialog.shutdown();
    }

    #[test]
    fn table_workflow_paste_open_filter_sort_copy_and_save_are_explicit() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source_path = directory.path().join("source.csv");
        let source_text = "name,group,rank,note\nalpha,keep,2,\"tab\tinside\"\nbravo,drop,10,excluded\ngamma,keep,3,\"line\nbreak\"\ndelta,keep,1,\"quote \"\"ok\"\"\"\n";
        let source_bytes = source_text.as_bytes();
        fs::write(&source_path, source_bytes).expect("write source fixture");

        let malformed_path = directory.path().join("malformed.csv");
        let malformed_text = "name,value\n\"unterminated";
        let malformed_bytes = malformed_text.as_bytes();
        fs::write(&malformed_path, malformed_bytes).expect("write malformed fixture");

        let save_path = directory.path().join("selection.json");
        let clipboard = ClipboardProbe::default();
        *clipboard
            .read_value
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = "name,value\npasted,1\n".to_owned();

        let context = egui::Context::default();
        let mut dialog = TableUtilityDialogState::default();
        dialog.set_clipboard_backend_for_test(Arc::new(clipboard.clone()));
        dialog.open();
        dialog.session.open(&context).expect("start table worker");
        let defaults = dialog.session.parse_options();
        assert_eq!(defaults.delimiter, DelimiterChoice::Auto);
        assert!(!defaults.has_header);
        assert_eq!(defaults.row_width, RowWidthPolicy::Lenient);
        dialog.session.set_has_header(true);

        // The first Window frame creates its area; the following real button
        // response is used for the explicit clipboard action.
        table_frame(&context, &mut dialog, Vec::new());
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 0);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);
        click_test_widget(&context, &mut dialog, |state| state.test_paste_response);
        let pasted = await_dialog(&context, &mut dialog, |snapshot| {
            snapshot.source.as_ref() == "name,value\npasted,1\n"
                && matches!(&snapshot.parse_state, TableParseState::Ready(_))
                && matches!(&snapshot.projection_state, TableProjectionState::Ready(_))
        });
        assert_eq!(pasted.source.as_ref(), "name,value\npasted,1\n");
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);

        // Open File uses the same rendered action and bounded session worker;
        // the test supplies only the chooser result, never writes the source.
        dialog.test_open_file_path = Some(source_path.clone());
        click_test_widget(&context, &mut dialog, |state| state.test_open_response);
        let canonical_source = fs::canonicalize(&source_path).expect("canonical source path");
        let loaded = await_dialog(&context, &mut dialog, |snapshot| {
            snapshot.source_path.as_ref() == Some(&canonical_source)
                && matches!(&snapshot.source_load_state, TableSourceLoadState::Idle)
                && matches!(&snapshot.parse_state, TableParseState::Ready(_))
                && matches!(&snapshot.projection_state, TableProjectionState::Ready(_))
        });
        let loaded_document = loaded.document.as_ref().expect("file parsed");
        let loaded_document_id = loaded_document.id();
        assert_eq!(loaded.source.as_ref().as_bytes(), source_bytes);
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);

        dialog.session.set_filter("keep".to_owned());
        dialog.session.set_sort(Some(TableSort {
            column: 2,
            direction: TableSortDirection::Descending,
            mode: TableSortMode::AutoNumeric,
        }));
        let filtered = await_dialog(&context, &mut dialog, |snapshot| {
            snapshot.view_revision != loaded.view_revision
                && matches!(snapshot.projection_state, TableProjectionState::Ready(_))
        });
        let document = filtered.document.as_ref().expect("last-good file document");
        let projection = filtered.projection.as_ref().expect("filtered projection");
        let displayed_names = projection
            .visible_rows()
            .iter()
            .map(|id| document.row(*id).expect("visible row").cells()[0].as_str())
            .collect::<Vec<_>>();
        assert_eq!(displayed_names, ["gamma", "alpha", "delta"]);
        let selected = projection
            .visible_rows()
            .iter()
            .copied()
            .filter(|id| {
                matches!(
                    document.row(*id).expect("visible row").cells()[0].as_str(),
                    "gamma" | "alpha"
                )
            })
            .collect::<std::collections::HashSet<_>>();
        dialog
            .session
            .set_selection(Some(selected), std::collections::HashSet::from([0, 3]));
        dialog.include_headers = true;
        dialog.json_shape = TableJsonShape::Arrays;
        let selected_snapshot = dialog.session.snapshot();
        assert_eq!(selected_snapshot.selected_visible_row_count, 2);
        for _ in 0..3 {
            table_frame(&context, &mut dialog, Vec::new());
        }
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);

        click_test_widget(&context, &mut dialog, |state| state.test_copy_tsv_response);
        let expected_tsv = "name\tnote\ngamma\t\"line\nbreak\"\nalpha\t\"tab\tinside\"\n";
        await_dialog(&context, &mut dialog, |_| {
            !clipboard
                .written_values
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_empty()
        });
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(clipboard_writes(&clipboard), vec![expected_tsv.to_owned()]);

        click_test_widget(&context, &mut dialog, |state| state.test_copy_json_response);
        let expected_json = "[[\"gamma\",\"line\\nbreak\"],[\"alpha\",\"tab\\tinside\"]]";
        await_dialog(&context, &mut dialog, |_| {
            clipboard
                .written_values
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .len()
                >= 2
        });
        assert_eq!(
            clipboard_writes(&clipboard),
            vec![expected_tsv.to_owned(), expected_json.to_owned()]
        );

        dialog.test_save_json_path = Some(save_path.clone());
        click_test_widget(&context, &mut dialog, |state| state.test_save_json_response);
        await_dialog(&context, &mut dialog, |_| save_path.exists());
        assert_eq!(
            fs::read(&save_path)
                .expect("captured JSON export")
                .as_slice(),
            expected_json.as_bytes()
        );
        assert_eq!(
            fs::read(&source_path)
                .expect("source remains read-only")
                .as_slice(),
            source_bytes
        );
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(
            clipboard_writes(&clipboard),
            vec![expected_tsv.to_owned(), expected_json.to_owned()]
        );

        // A malformed explicit file read leaves the accepted document and
        // source bytes untouched, but invalidates export eligibility.
        dialog.test_open_file_path = Some(malformed_path.clone());
        click_test_widget(&context, &mut dialog, |state| state.test_open_response);
        let failed = await_dialog(&context, &mut dialog, |snapshot| {
            matches!(&snapshot.parse_state, TableParseState::Failed { .. })
        });
        assert_eq!(
            failed.document.as_ref().map(|document| document.id()),
            Some(loaded_document_id)
        );
        assert!(!failed.accepted_options_are_current());
        assert!(!failed.can_export());
        assert_eq!(failed.source.as_ref(), malformed_text);
        assert!(
            dialog
                .session
                .request_export(TableExportKind::Tsv, true)
                .is_err()
        );
        assert_eq!(
            fs::read(&source_path).expect("source unchanged").as_slice(),
            source_bytes
        );
        assert_eq!(
            fs::read(&malformed_path)
                .expect("malformed source unchanged")
                .as_slice(),
            malformed_bytes
        );
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(
            clipboard_writes(&clipboard),
            vec![expected_tsv.to_owned(), expected_json.to_owned()]
        );
        dialog.shutdown();
    }

    #[test]
    fn table_workflow_virtualizes_rows_and_idle_frames_submit_no_jobs() {
        let context = egui::Context::default();
        let source = format!(
            "name,value\n{}",
            (0..5_000)
                .map(|index| format!("row-{index},{}\n", index % 100))
                .collect::<String>()
        );
        let mut dialog = TableUtilityDialogState::default();
        prepare_dialog_table(&context, &mut dialog, &source);

        // The first pass lays out the Window; row-range observations below
        // come from the actual `show_rows` callback on the next pass.
        table_frame(&context, &mut dialog, Vec::new());
        table_frame(&context, &mut dialog, Vec::new());
        let snapshot = dialog.session.snapshot();
        let document_id = snapshot.document.as_ref().expect("parsed document").id();
        let projection_id = snapshot
            .projection
            .as_ref()
            .expect("current projection")
            .id();
        let first_visible = snapshot
            .projection
            .as_ref()
            .expect("current projection")
            .visible_rows()[0];
        let range = dialog
            .test_rendered_row_range
            .expect("actual virtual row range was rendered");
        assert_eq!(dialog.test_visible_row_count, 5_000);
        assert!(range.1 > range.0);
        assert!(
            range.1 - range.0 < 32,
            "only a viewport-sized row range is rendered"
        );
        let counts = dialog.session.test_job_counts();
        assert!(counts.parse_submitted > 0 && counts.parse_executed > 0);
        assert!(counts.project_submitted > 0 && counts.project_executed > 0);

        for _ in 0..4 {
            table_frame(&context, &mut dialog, Vec::new());
        }
        let after_idle = dialog.session.snapshot();
        assert_eq!(dialog.session.test_job_counts(), counts);
        assert_eq!(
            after_idle.document.as_ref().map(|document| document.id()),
            Some(document_id)
        );
        assert_eq!(
            after_idle.projection.as_ref().map(|view| view.id()),
            Some(projection_id)
        );
        assert_eq!(
            after_idle.projection.as_ref().unwrap().visible_rows()[0],
            first_visible
        );
        dialog.shutdown();
    }

    #[test]
    fn table_workflow_large_source_keeps_full_input_outside_text_edit() {
        let context = egui::Context::default();
        let payload = "x".repeat(crate::table_utility_session::TABLE_INLINE_EDIT_MAX_BYTES + 1);
        let source = format!("value\n{payload}\n");
        let mut dialog = TableUtilityDialogState::default();
        prepare_dialog_table(&context, &mut dialog, &source);

        table_frame(&context, &mut dialog, Vec::new());
        table_frame(&context, &mut dialog, Vec::new());
        let snapshot = dialog.session.snapshot();
        assert_eq!(snapshot.source.as_ref(), source);
        assert!(!snapshot.source_is_editable());
        assert!(dialog.test_source_editor_response.is_none());
        assert_eq!(dialog.test_source_preview_chars, 2_049);
        assert!(dialog.test_source_preview_truncated);
        assert_eq!(snapshot.document.as_ref().unwrap().rows().len(), 1);
        dialog.shutdown();
    }

    #[test]
    fn table_file_save_refuses_to_replace_the_opened_source() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("source.tsv");
        fs::write(&source, b"a\tb\n1\t2\n").expect("source fixture");
        let canonical_source = fs::canonicalize(&source).expect("canonical source");
        let refusal =
            save_export_target(Some(&canonical_source), &source, b"new JSON payload", true);
        assert!(refusal.unwrap_err().contains("read-only"));
        assert_eq!(
            fs::read(&source).expect("source remains intact"),
            b"a\tb\n1\t2\n"
        );
    }

    #[test]
    fn table_file_save_requires_confirmation_and_publishes_the_captured_output() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("selected.json");
        fs::write(&target, b"old contents").expect("existing destination");

        let pending = save_export_target(None, &target, br#"["new"]"#, false)
            .expect("existing output requests confirmation");
        assert_eq!(pending, SaveOutcome::OverwriteRequired);
        assert_eq!(
            fs::read(&target).expect("unchanged before confirmation"),
            b"old contents"
        );

        let saved = save_export_target(None, &target, br#"["captured"]"#, true)
            .expect("confirmed output is replaced");
        assert_eq!(saved, SaveOutcome::Saved);
        assert_eq!(fs::read(&target).expect("saved file"), br#"["captured"]"#);

        let new_target = directory.path().join("new.json");
        assert_eq!(
            save_export_target(None, &new_target, br#"["new"]"#, false)
                .expect("new output is created without overwrite confirmation"),
            SaveOutcome::Saved
        );
        assert_eq!(fs::read(new_target).expect("created file"), br#"["new"]"#);
    }

    #[test]
    fn table_dialog_owns_combo_popup_and_uses_one_horizontal_results_viewport() {
        let context = egui::Context::default();
        let mut dialog = TableUtilityDialogState::default();
        prepare_dialog_table(&context, &mut dialog, "a,b,c,d,e,f,g,h\n1,2,3,4,5,6,7,8\n");
        for _ in 0..3 {
            table_frame(&context, &mut dialog, Vec::new());
        }

        let anchor = dialog.combo_anchor_ids[0].expect("delimiter ComboBox response");
        let anchor_response =
            table_frame_with_response(&context, &mut dialog, Vec::new(), Some(anchor))
                .expect("the current delimiter ComboBox response is rendered");
        assert!(anchor_response.enabled());
        click_response(&context, &mut dialog, anchor);
        table_frame(&context, &mut dialog, Vec::new());
        assert!(
            dialog.owned_combo_popup_is_current(&context),
            "the opened delimiter popup is the current child of this Table Utility window"
        );
        let tab_option = dialog
            .delimiter_option_response_id_for_test(DelimiterChoice::Explicit(TableDelimiter::Tab))
            .expect("the real TSV option response is rendered");
        let tab_option_response =
            table_frame_with_response(&context, &mut dialog, Vec::new(), Some(tab_option))
                .expect("the current TSV option response is rendered before the overlay");
        assert!(
            tab_option_response.enabled(),
            "the actual current TSV option response is enabled while its popup is open"
        );

        let foreign_overlay_id = egui::Id::new("foreign-table-dialog-overlay");
        let foreign_layer = egui::LayerId::new(egui::Order::Foreground, foreign_overlay_id);
        let foreign_overlay = context.run_ui(table_input(Vec::new()), |ctx| {
            dialog.show_with_jump_mode(
                ctx,
                None,
                None,
                crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                true,
                true,
                true,
            );
            egui::Area::new(foreign_overlay_id)
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(720.0, 40.0))
                .show(ctx, |ui| ui.label("Foreign overlay"));
        });
        foreign_overlay.drop_without_applying_deltas();

        let mut foreign_popup_is_current = None;
        let mut foreign_layer_was_visible = None;
        let foreign_overlay = context.run_ui(table_input(Vec::new()), |ctx| {
            dialog.show_with_jump_mode(
                ctx,
                None,
                None,
                crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                true,
                true,
                true,
            );
            egui::Area::new(foreign_overlay_id)
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(720.0, 40.0))
                .show(ctx, |ui| ui.label("Foreign overlay"));
            foreign_layer_was_visible =
                Some(ctx.memory(|memory| memory.areas().visible_last_frame(&foreign_layer)));
            foreign_popup_is_current = Some(dialog.owned_combo_popup_is_current(ctx));
        });
        foreign_overlay.drop_without_applying_deltas();
        assert!(
            foreign_layer_was_visible == Some(true),
            "the foreign layer is published as visible before ownership is evaluated"
        );
        assert_eq!(
            foreign_popup_is_current,
            Some(false),
            "an unrelated foreground overlay blocks the table combo owner"
        );
        table_frame(&context, &mut dialog, Vec::new());
        let (recovered_owner, recovered_anchor, recovered_option) =
            table_frame_with_combo_observation(&context, &mut dialog, anchor, tab_option);
        assert!(
            recovered_owner,
            "the owned combo recovers when the foreign overlay is gone"
        );
        assert!(
            recovered_anchor.is_some_and(|response| response.enabled()),
            "the recovered current anchor response is enabled"
        );
        assert!(
            recovered_option.is_some_and(|response| response.enabled()),
            "the recovered current TSV option response is enabled"
        );

        let tab_option_response = click_response(&context, &mut dialog, tab_option);
        assert!(
            tab_option_response.enabled(),
            "the recovered current TSV option response is enabled before activation"
        );

        assert_eq!(
            dialog.session.parse_options().delimiter,
            DelimiterChoice::Explicit(TableDelimiter::Tab),
            "a real selection in the owned popup updates the parse draft"
        );

        dialog
            .session
            .replace_source("a\tb\tc\td\te\tf\tg\th\n1\t2\t3\t4\t5\t6\t7\t8\n".to_owned())
            .expect("replace the test source with eight tab-separated columns");
        dialog.session.request_parse();
        wait_for_test_snapshot(&mut dialog.session).expect("TSV draft reparses");
        for _ in 0..2 {
            table_frame(&context, &mut dialog, Vec::new());
        }
        let (scroll_id, content_size, inner_rect, _) = dialog.test_results_layout();
        let inner_rect = inner_rect.expect("one horizontal results viewport");
        assert!(
            content_size.x > inner_rect.width(),
            "columns need horizontal scrolling"
        );
        let scroll_id = scroll_id.expect("the actual results ScrollArea has a stable id");
        let requested_offset = 220.0_f32.min((content_size.x - inner_rect.width()).max(0.0));
        assert!(requested_offset > 0.0);
        let mut scroll = egui::scroll_area::State::load(&context, scroll_id)
            .expect("the results ScrollArea retained its state");
        scroll.offset.x = requested_offset;
        scroll.store(&context, scroll_id);
        table_frame(&context, &mut dialog, Vec::new());

        let (rendered_scroll_id, content_size, inner_rect, offset) = dialog.test_results_layout();
        assert_eq!(rendered_scroll_id, Some(scroll_id));
        let inner_rect = inner_rect.expect("viewport remains visible");
        assert!(content_size.x > inner_rect.width());
        assert!(
            offset > 0.0,
            "the shared results viewport applies its bounded horizontal offset"
        );
        let header = dialog
            .test_first_header_column_rect
            .expect("first header response painted");
        let data = dialog
            .test_first_data_column_rect
            .expect("first visible data cell painted");
        assert!(
            (header.left() - data.left()).abs() < 1.0,
            "header and data stay aligned under the same horizontal offset"
        );
        dialog.shutdown();
    }

    #[test]
    fn table_dialog_overwrite_buttons_revalidate_the_captured_export() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("existing.json");
        fs::write(&target, b"old output").expect("existing destination");
        let context = egui::Context::default();
        let mut dialog = TableUtilityDialogState::default();
        dialog
            .test_prepare_overwrite_confirmation(&context, target.clone())
            .expect("prepare an existing-file confirmation from a real export receipt");
        for _ in 0..3 {
            table_frame(&context, &mut dialog, Vec::new());
        }
        assert!(dialog.overwrite_confirmation_layer_is_current(&context));
        let replace = dialog
            .overwrite_response_id_for_test(true)
            .expect("Replace button response");
        let replace_response =
            table_frame_with_response(&context, &mut dialog, Vec::new(), Some(replace))
                .expect("the current Replace response is rendered");
        assert!(replace_response.enabled());
        click_response(&context, &mut dialog, replace);
        assert!(dialog.pending_overwrite.is_none());
        assert_eq!(
            fs::read(&target).expect("replaced output"),
            br#"[["alpha"]]"#
        );
        dialog.shutdown();

        fs::write(&target, b"unchanged stale destination").expect("reset destination");
        let context = egui::Context::default();
        let mut dialog = TableUtilityDialogState::default();
        dialog
            .test_prepare_overwrite_confirmation(&context, target.clone())
            .expect("prepare the stale receipt case");
        for _ in 0..3 {
            table_frame(&context, &mut dialog, Vec::new());
        }
        dialog
            .session
            .inline_source_changed("name,value\nbeta,2\n".to_owned())
            .expect("source change revokes the captured receipt");
        let pending_receipt = &dialog
            .pending_overwrite
            .as_ref()
            .expect("stale confirmation retains its captured receipt")
            .receipt;
        assert!(dialog.session.snapshot().source_dirty);
        assert!(
            !dialog.session.export_receipt_is_current(pending_receipt),
            "the exact receipt is stale as soon as its source draft changes"
        );
        let stale_replace = dialog
            .overwrite_response_id_for_test(true)
            .expect("stale Replace response remains visible");
        let stale_replace_response =
            table_frame_with_response(&context, &mut dialog, Vec::new(), Some(stale_replace))
                .expect("the current stale Replace response is rendered");
        assert!(
            !stale_replace_response.enabled(),
            "a visible Replace response cannot use a stale export receipt"
        );
        let cancel = dialog
            .overwrite_response_id_for_test(false)
            .expect("Cancel response remains available for a stale receipt");
        let cancel_response =
            table_frame_with_response(&context, &mut dialog, Vec::new(), Some(cancel))
                .expect("the current Cancel response is rendered");
        assert!(cancel_response.enabled());
        click_response(&context, &mut dialog, cancel);
        assert!(dialog.pending_overwrite.is_none());
        assert_eq!(
            fs::read(&target).expect("stale confirmation cannot replace destination"),
            b"unchanged stale destination"
        );
        dialog.shutdown();
    }
}
