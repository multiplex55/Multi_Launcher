use crate::diff::folder_runtime::{
    FolderComparisonIdentity, FolderOperationContext, FolderOperationFeedback,
};
use crate::diff::model::{DiffView, DiffWorkspace, TextCompareOrigin, TextCompareState};
use crate::diff::query::DiffOpenPayload;
use crate::gui::theme::widgets::{StatusTone, semantic_status};
use eframe::egui;
use std::collections::HashMap;
use std::collections::HashSet;
mod binary_view;
mod export;
mod folder_rules_dialog;
mod folder_table;
mod folder_view;
mod operation_preview;
mod rules_dialog;
mod text_view;

/// Window builders take outer sizes in egui 0.36, while Diff persists body sizes.
fn window_chrome(ctx: &egui::Context) -> egui::Vec2 {
    let style = ctx.global_style();
    let frame = egui::Frame::window(&style);
    let heading_height =
        ctx.fonts_mut(|fonts| fonts.row_height(&egui::TextStyle::Heading.resolve(&style)));
    // AtomLayout allocates the laid-out text galley, which can be taller than
    // the fractional font row metric after glyph/layout rounding. Reuse its
    // WidgetText preparation so font overrides and extra line spacing agree.
    let title_height = egui::WidgetText::from("Diff")
        .into_galley_impl(
            ctx,
            &style,
            egui::text::TextWrapping::from_wrap_mode_and_width(
                egui::TextWrapMode::Truncate,
                f32::INFINITY,
            ),
            egui::TextStyle::Heading.into(),
            style.override_text_valign.unwrap_or(egui::Align::Min),
        )
        .size()
        .y
        .max(heading_height);
    // The title's atom frame retains the window inner margin. Its cursor advance
    // also includes the bottom title stroke; body margins are counted separately.
    frame.total_margin().sum()
        + egui::vec2(
            0.0,
            title_height + frame.inner_margin.sum().y + frame.total_margin().bottom
                - f32::from(frame.inner_margin.bottom),
        )
}

fn inner_work_area(screen: [f32; 4], chrome: egui::Vec2) -> [f32; 4] {
    [
        screen[0],
        screen[1],
        (screen[2] - chrome.x).max(screen[0]),
        (screen[3] - chrome.y).max(screen[1]),
    ]
}

/// Captures a complete, persistable inner size and outer position, or rejects
/// the observation atomically.
fn runtime_window_geometry(
    inner_size: egui::Vec2,
    outer_position: egui::Pos2,
    screen: [f32; 4],
) -> Option<([f32; 2], [f32; 2])> {
    let geometry = [
        inner_size.x,
        inner_size.y,
        outer_position.x,
        outer_position.y,
    ];
    let size = [inner_size.x, inner_size.y];
    let minimum = crate::diff::model::diff_window_min_size(screen);
    (geometry.iter().all(|value| value.is_finite())
        && size[0] >= minimum[0]
        && size[1] >= minimum[1])
        .then_some((size, [outer_position.x, outer_position.y]))
}

fn render_retained_folder(
    state: &mut crate::diff::model::FolderCompareState,
    render: impl FnOnce(&mut crate::diff::model::FolderCompareState) -> folder_view::FolderViewAction,
) -> folder_view::FolderViewAction {
    render(state)
}

struct WorkspaceRenderOutcome {
    folder_action: folder_view::FolderViewAction,
    text_action: Option<text_view::TextViewAction>,
    error: Option<String>,
    recent_to_open: Option<crate::diff::persistence::DisplayPathPairV1>,
}

#[derive(Clone, Copy)]
enum TextInputPurpose {
    NewComparison,
    EditInputs { view_id: u64 },
}

struct TextInputDraft {
    purpose: TextInputPurpose,
    left: String,
    right: String,
    expected_sources: Option<(TextInputSourceSnapshot, TextInputSourceSnapshot)>,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextInputSourceSnapshot {
    source_id: u64,
    revision: u64,
}

impl TextInputSourceSnapshot {
    fn capture(document: &crate::diff::text_file::TextDocument) -> Self {
        Self {
            source_id: document.source_id(),
            revision: document.revision,
        }
    }
}

fn paste_memory_input(
    accepted: &mut String,
    read_clipboard: impl FnOnce() -> Result<String, String>,
) -> Result<(), String> {
    let text = read_clipboard()?;
    crate::diff::model::validate_memory_text(&text)?;
    *accepted = text;
    Ok(())
}

fn configure_text_model(
    model: &mut crate::diff::model::TextViewModel,
    settings: &crate::diff::settings::DiffConfigV1,
    persistence: &crate::diff::persistence::DiffPersistenceV1,
) {
    let mut rules = model.rules.clone();
    rules.ignore_all_whitespace = settings.ignore_whitespace;
    rules.case_sensitive = settings.case_sensitive;
    rules.replacements = persistence
        .replacement_rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| crate::diff::text_compare::RegexReplacement {
            pattern: rule.pattern.clone(),
            replacement: rule.replacement.clone(),
        })
        .collect();
    rules.unimportant_sections = persistence
        .unimportant_section_rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| rule.pattern.clone())
        .collect();
    model.set_rules(rules);
}

type RestoredTextPreferences = (bool, bool, bool, bool, u8, u8);

fn apply_restored_text_preferences(
    model: &mut crate::diff::model::TextViewModel,
    (details, whitespace, vertical, horizontal, projection, context): RestoredTextPreferences,
) {
    model.text_details_open = details;
    model.visible_whitespace = whitespace;
    model.scroll.set_sync(vertical, horizontal);
    let mode = match projection {
        1 => crate::diff::text_compare::RowProjectionMode::DifferencesOnly,
        2 => crate::diff::text_compare::RowProjectionMode::IdenticalOnly,
        _ => crate::diff::text_compare::RowProjectionMode::All,
    };
    model.set_row_projection(mode, context as usize);
}

#[derive(Clone, Copy, Debug)]
struct DiffViewport {
    rect: egui::Rect,
    size: egui::Vec2,
}

impl DiffViewport {
    fn remaining(ui: &egui::Ui) -> Self {
        let available = ui.available_rect_before_wrap();
        let bounded = |value: f32| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        };
        let size = egui::vec2(bounded(available.width()), bounded(available.height()));
        Self {
            rect: egui::Rect::from_min_size(available.min, size),
            size,
        }
    }
}

fn allocate_viewport<R>(
    ui: &mut egui::Ui,
    viewport: DiffViewport,
    render: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    // The parent owns only this fixed allocation. Descendants can grow their
    // scroll extents, but cannot feed a larger minimum back into the Window.
    let response = ui.allocate_rect(viewport.rect, egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(viewport.rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(viewport.rect));
    let inner = render(&mut child);
    egui::InnerResponse { inner, response }
}

fn allocate_remaining_workspace<R>(
    ui: &mut egui::Ui,
    render: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let viewport = DiffViewport::remaining(ui);
    allocate_viewport(ui, viewport, render)
}

fn bounded_message(ui: &mut egui::Ui, text: &str, max_height: f32) {
    egui::ScrollArea::both()
        .max_height(max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            semantic_status(ui, StatusTone::Error, text, None);
        });
}

#[derive(Default)]
pub struct DiffDialogState {
    pub open: bool,
    pub workspace: DiffWorkspace,
    text_views: HashMap<u64, crate::diff::model::TextViewModel>,
    folder_runtimes: HashMap<u64, crate::diff::folder_runtime::FolderRuntime>,
    operation_preview: Option<operation_preview::OperationPreview>,
    binary_views: HashMap<u64, crate::diff::binary_compare::BinaryViewModel>,
    watch_views: HashMap<u64, crate::diff::watch::ViewWatchRuntime>,
    restored_text_preferences: HashMap<u64, RestoredTextPreferences>,
    text_input: Option<TextInputDraft>,
    close_prompt: bool,
    pub persistence: crate::diff::persistence::DiffPersistenceV1,
}

impl DiffDialogState {
    fn swap_sides(&mut self) {
        let id = self.workspace.current_view.id;
        let result = if let Some(model) = self.text_views.get_mut(&id) {
            model.swap_sides();
            Ok(())
        } else if let Some(model) = self.binary_views.get_mut(&id) {
            model.swap_sides()
        } else {
            Ok(())
        };
        if let Err(e) = result.and_then(|_| self.workspace.swap_sides()) {
            self.workspace.error = Some(e);
            return;
        }
        self.watch_views.remove(&id);
        if matches!(self.workspace.current_view.view, DiffView::FolderCompare(_)) {
            self.folder_runtimes.remove(&id);
        }
        if (matches!(&self.workspace.current_view.view, DiffView::TextCompare(state) if state.relative_path.is_some())
            || matches!(&self.workspace.current_view.view, DiffView::BinaryCompare(state) if state.relative_path.is_some()))
            && let Some(parent) = self.workspace.navigation_stack.last()
        {
            self.folder_runtimes.remove(&parent.id);
            self.watch_views.remove(&parent.id);
        }
        self.reconcile_runtime_resources();
    }

    fn reload_files(&mut self) {
        let id = self.workspace.current_view.id;
        let result = match &self.workspace.current_view.view {
            DiffView::TextCompare(_) => self
                .text_views
                .get_mut(&id)
                .ok_or_else(|| "Text view is not loaded".into())
                .and_then(|m| m.reload_files()),
            DiffView::BinaryCompare(state) => self
                .binary_views
                .get_mut(&id)
                .ok_or_else(|| "Binary view is not loaded".into())
                .and_then(|m| m.refresh_external(state)),
            _ => Err("Reload Files is available for Text and Binary comparisons".into()),
        };
        if let Err(e) = result {
            self.workspace.error = Some(e);
        }
    }

    fn recompare(&mut self) {
        let id = self.workspace.current_view.id;
        let result = if let Some(m) = self.text_views.get_mut(&id) {
            m.recompare();
            Ok(())
        } else if let Some(m) = self.binary_views.get_mut(&id) {
            m.recompare()
        } else {
            Err("Recompare is available for Text and Binary comparisons".into())
        };
        if let Err(e) = result {
            self.workspace.error = Some(e);
        }
    }

    fn begin_text_input(&mut self, purpose: TextInputPurpose) {
        let (left, right, expected_sources) = match purpose {
            TextInputPurpose::NewComparison => (String::new(), String::new(), None),
            TextInputPurpose::EditInputs { view_id } => {
                let Some(model) = self.text_views.get(&view_id) else {
                    self.workspace.error = Some("The in-memory text view is not loaded".into());
                    return;
                };
                if model.origin != TextCompareOrigin::Memory {
                    self.workspace.error =
                        Some("Only in-memory text comparisons have editable inputs".into());
                    return;
                }
                (
                    model.left.source().to_owned(),
                    model.right.source().to_owned(),
                    Some((
                        TextInputSourceSnapshot::capture(&model.left),
                        TextInputSourceSnapshot::capture(&model.right),
                    )),
                )
            }
        };
        self.text_input = Some(TextInputDraft {
            purpose,
            left,
            right,
            expected_sources,
            error: None,
        });
    }

    fn compare_selection_with_clipboard_using(
        &mut self,
        view_id: u64,
        read_clipboard: impl FnOnce() -> Result<String, String>,
    ) -> Result<(), String> {
        let origin = match &self.workspace.current_view.view {
            DiffView::TextCompare(state) if self.workspace.current_view.id == view_id => {
                state.origin
            }
            _ => return Err("The selected text comparison is no longer active".into()),
        };
        let model = self
            .text_views
            .get(&view_id)
            .ok_or_else(|| "The selected text view is not loaded".to_string())?;
        if model.origin != origin {
            return Err("The selected text view no longer matches its comparison".into());
        }
        let selected = model.selected_raw_source_text()?;
        crate::diff::model::validate_memory_text(&selected.text)?;

        let clipboard = read_clipboard()?;
        crate::diff::model::validate_memory_text(&clipboard)?;

        if self.workspace.current_view.id != view_id
            || !matches!(
                &self.workspace.current_view.view,
                DiffView::TextCompare(state) if state.origin == origin
            )
        {
            return Err("The selected text comparison changed while reading the clipboard".into());
        }
        let model = self
            .text_views
            .get(&view_id)
            .ok_or_else(|| "The in-memory text view is no longer available".to_string())?;
        let source = match selected.side {
            crate::diff::model::DiffSide::Left => &model.left,
            crate::diff::model::DiffSide::Right => &model.right,
        };
        if source.source_id() != selected.source_id || source.revision != selected.revision {
            return Err("The selected source changed while reading the clipboard".into());
        }

        let empty_clipboard = clipboard.is_empty();
        let mut new_model = crate::diff::model::TextViewModel::new_memory(
            selected.text,
            clipboard,
            &self.workspace.settings,
        )?;
        configure_text_model(&mut new_model, &self.workspace.settings, &self.persistence);
        let new_view_id = self.workspace.push_memory_text_compare();
        self.text_views.insert(new_view_id, new_model);
        self.watch_views.remove(&new_view_id);
        self.workspace.error = None;
        self.workspace.status = Some(if empty_clipboard {
            "Selected text compared with empty clipboard text; empty text is valid".into()
        } else {
            "Selected text compared with clipboard text".into()
        });
        self.reconcile_runtime_resources();
        Ok(())
    }

    fn apply_text_input(&mut self, draft: TextInputDraft) -> Result<(), (TextInputDraft, String)> {
        if let Err(error) = crate::diff::model::validate_memory_text(&draft.left)
            .and_then(|()| crate::diff::model::validate_memory_text(&draft.right))
        {
            return Err((draft, error));
        }
        match draft.purpose {
            TextInputPurpose::NewComparison => {
                let mut model = match crate::diff::model::TextViewModel::new_memory(
                    draft.left.clone(),
                    draft.right.clone(),
                    &self.workspace.settings,
                ) {
                    Ok(model) => model,
                    Err(error) => return Err((draft, error)),
                };
                configure_text_model(&mut model, &self.workspace.settings, &self.persistence);
                let view_id = self.workspace.push_memory_text_compare();
                self.text_views.insert(view_id, model);
                self.watch_views.remove(&view_id);
                self.workspace.error = None;
                self.workspace.status = Some("In-memory text comparison ready".into());
                self.reconcile_runtime_resources();
                Ok(())
            }
            TextInputPurpose::EditInputs { view_id } => {
                if self.workspace.current_view.id != view_id {
                    return Err((
                        draft,
                        "The active comparison changed while Edit Inputs was open. Reopen it to edit the current comparison.".into(),
                    ));
                }
                let Some(model) = self.text_views.get_mut(&view_id) else {
                    return Err((
                        draft,
                        "The in-memory text view is no longer available".into(),
                    ));
                };
                let Some(expected_sources) = draft.expected_sources else {
                    return Err((
                        draft,
                        "The Edit Inputs draft is no longer tied to its source documents. Reopen it to continue.".into(),
                    ));
                };
                let current_sources = (
                    TextInputSourceSnapshot::capture(&model.left),
                    TextInputSourceSnapshot::capture(&model.right),
                );
                if current_sources != expected_sources {
                    return Err((
                        draft,
                        "The comparison sources changed while Edit Inputs was open. Reopen it to review the latest text.".into(),
                    ));
                }
                match model.replace_memory_sources(draft.left.clone(), draft.right.clone()) {
                    Ok(_) => {
                        self.workspace.error = None;
                        self.workspace.status = Some("In-memory text inputs updated".into());
                        Ok(())
                    }
                    Err(error) => Err((draft, error)),
                }
            }
        }
    }

    fn show_text_input(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.text_input.as_mut() else {
            return;
        };
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;
        let title = match draft.purpose {
            TextInputPurpose::NewComparison => "Compare Text",
            TextInputPurpose::EditInputs { .. } => "Edit Text Inputs",
        };
        let identity = match draft.purpose {
            TextInputPurpose::NewComparison => 0,
            TextInputPurpose::EditInputs { view_id } => view_id,
        };
        egui::Window::new(title)
            .id(egui::Id::new((self.workspace.workspace_id, "text_input", identity)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(760.0, 540.0))
            .show(ctx, |ui| {
                ui.label("Enter text for both sides. Empty text is allowed.");
                ui.label(
                    "Each side is limited to 8 MiB and 100,000 lines. Oversized input is rejected without truncation.",
                );
                ui.label(
                    "Text remains in Diff memory until you explicitly use Save As or Export.",
                );
                ui.columns(2, |columns| {
                    for (side, input, label) in [
                        (crate::diff::model::DiffSide::Left, &mut draft.left, "Left text"),
                        (
                            crate::diff::model::DiffSide::Right,
                            &mut draft.right,
                            "Right text",
                        ),
                    ] {
                        let column = if side == crate::diff::model::DiffSide::Left {
                            &mut columns[0]
                        } else {
                            &mut columns[1]
                        };
                        column.label(label);
                        column.add(
                            egui::TextEdit::multiline(input)
                                .id(egui::Id::new((
                                    self.workspace.workspace_id,
                                    "memory_text_source",
                                    identity,
                                    side,
                                )))
                                .desired_rows(16)
                                .desired_width(f32::INFINITY)
                                .code_editor(),
                        );
                        column.horizontal(|ui| {
                            if ui.button(format!("Paste {side:?}")).clicked() {
                                let result = paste_memory_input(input, || {
                                    crate::actions::clipboard::get_text().map_err(|_| {
                                        "Clipboard is unavailable or does not contain plain text"
                                            .to_string()
                                    })
                                });
                                match result {
                                    Ok(()) => draft.error = None,
                                    Err(error) => draft.error = Some(error),
                                }
                            }
                            ui.label(format!("{} bytes", input.len()));
                        });
                    }
                });
                if let Some(error) = &draft.error {
                    semantic_status(ui, StatusTone::Error, error, None);
                }
                ui.horizontal(|ui| {
                    let label = match draft.purpose {
                        TextInputPurpose::NewComparison => "Compare",
                        TextInputPurpose::EditInputs { .. } => "Apply Inputs",
                    };
                    if ui.button(label).clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                        ui.close();
                    }
                });
            });
        if submit {
            if let Some(draft) = self.text_input.take() {
                match self.apply_text_input(draft) {
                    Ok(()) => {}
                    Err((mut draft, error)) => {
                        draft.error = Some(error);
                        self.text_input = Some(draft);
                    }
                }
            }
        } else if cancel || !open {
            self.text_input = None;
        }
    }

    fn refresh_folder(&mut self) {
        let id = self.workspace.current_view.id;
        if let DiffView::FolderCompare(state) = &mut self.workspace.current_view.view {
            state.model = Default::default();
            state.left_scan_complete = false;
            state.right_scan_complete = false;
            state.stale_paths.clear();
            self.folder_runtimes.entry(id).or_default().prepare_rescan();
        } else {
            self.workspace.error =
                Some("Refresh Folder is available for Folder comparisons".into());
        }
    }

    fn ensure_memory_replacement_is_safe(&mut self) -> Result<(), String> {
        if self
            .text_views
            .values()
            .any(|model| model.origin == TextCompareOrigin::Memory && model.has_dirty())
        {
            let error = "Opening another comparison would discard edited in-memory text. Save As, or close Diff and choose Save modified, Discard, or Cancel before replacing it.".to_string();
            self.workspace.error = Some(error.clone());
            Err(error)
        } else {
            Ok(())
        }
    }

    fn cancel_close_prompt(&mut self, open: &mut bool) {
        self.close_prompt = false;
        *open = true;
    }

    pub fn open_payload(&mut self, payload: DiffOpenPayload) -> Result<(), String> {
        self.ensure_memory_replacement_is_safe()?;
        self.open = true;
        self.clear_runtime_resources();
        self.workspace = DiffWorkspace::new(self.persistence.config.clone());
        match (payload.left, payload.right) {
            (Some(left), Some(right)) => self.open_and_record(left, right),
            (left, right) => self.workspace.open_invocation(left, right),
        }
    }

    fn open_and_record(&mut self, left: String, right: String) -> Result<(), String> {
        self.ensure_memory_replacement_is_safe()?;
        self.workspace.open_paths(left.clone(), right.clone())?;
        let mode = match self.workspace.current_view.view {
            DiffView::TextCompare(_) => crate::diff::persistence::ComparisonModeV1::Text,
            DiffView::BinaryCompare(_) => crate::diff::persistence::ComparisonModeV1::Binary,
            DiffView::FolderCompare(_) => crate::diff::persistence::ComparisonModeV1::Folder,
            DiffView::Start => return Err("comparison did not open".into()),
        };
        crate::diff::persistence::record_recent_mode(&mut self.persistence, left, right, mode);
        self.reconcile_runtime_resources();
        Ok(())
    }

    pub fn snapshot_session(
        &self,
        name: String,
    ) -> Result<crate::diff::persistence::SavedDiffSessionV1, String> {
        use crate::diff::persistence::{
            ComparisonModeV1, ContentComparisonModeV1, SavedDiffSessionV1,
        };
        if matches!(
            &self.workspace.current_view.view,
            DiffView::TextCompare(state) if state.origin == TextCompareOrigin::Memory
        ) {
            return Err(
                "In-memory text comparisons cannot be saved as named sessions; use Export or Save As"
                    .into(),
            );
        }
        let (mode, includes, excludes, display, content) = match &self.workspace.current_view.view {
            DiffView::TextCompare(_) => (
                ComparisonModeV1::Text,
                vec![],
                vec![],
                "all".into(),
                ContentComparisonModeV1::OnDemand,
            ),
            DiffView::BinaryCompare(_) => (
                ComparisonModeV1::Binary,
                vec![],
                vec![],
                "all".into(),
                ContentComparisonModeV1::OnDemand,
            ),
            DiffView::FolderCompare(folder) => (
                ComparisonModeV1::Folder,
                folder.applied_scan_rules.includes.clone(),
                folder.applied_scan_rules.excludes.clone(),
                folder_filter_name(&folder.display_filter).into(),
                match folder.content_comparison {
                    crate::diff::model::ContentComparisonMode::Metadata => {
                        ContentComparisonModeV1::Metadata
                    }
                    crate::diff::model::ContentComparisonMode::OnDemand => {
                        ContentComparisonModeV1::OnDemand
                    }
                    crate::diff::model::ContentComparisonMode::Always => {
                        ContentComparisonModeV1::Always
                    }
                },
            ),
            DiffView::Start => return Err("open a comparison before saving a session".into()),
        };
        let text_model = self.text_views.get(&self.workspace.current_view.id);
        let binary_model = self.binary_views.get(&self.workspace.current_view.id);
        let pane_split = text_model
            .map(|m| m.splitter)
            .or_else(|| binary_model.map(|m| m.splitter))
            .unwrap_or(self.workspace.settings.pane_split);
        let wrap_text = text_model.map_or(self.workspace.settings.wrap_text, |m| m.wrap);
        let syntax_highlighting =
            text_model.map_or(self.workspace.settings.syntax_highlighting, |m| m.syntax);
        let replacement_rules = text_model
            .map(|m| {
                m.rules
                    .replacements
                    .iter()
                    .enumerate()
                    .map(|(i, r)| crate::diff::settings::ReplacementRuleV1 {
                        id: format!("replacement-{i}"),
                        pattern: r.pattern.clone(),
                        replacement: r.replacement.clone(),
                        enabled: true,
                    })
                    .collect()
            })
            .unwrap_or_else(|| self.persistence.replacement_rules.clone());
        let unimportant_section_rules = text_model
            .map(|m| {
                m.rules
                    .unimportant_sections
                    .iter()
                    .enumerate()
                    .map(
                        |(i, pattern)| crate::diff::settings::UnimportantSectionRuleV1 {
                            id: format!("section-{i}"),
                            pattern: pattern.clone(),
                            enabled: true,
                        },
                    )
                    .collect()
            })
            .unwrap_or_else(|| self.persistence.unimportant_section_rules.clone());
        Ok(SavedDiffSessionV1 {
            id: String::new(),
            name,
            left: self.workspace.left_visible.clone(),
            right: self.workspace.right_visible.clone(),
            pane_split,
            wrap_text,
            syntax_highlighting,
            syntax_theme: self.workspace.settings.syntax_theme.clone(),
            comparison_mode: mode,
            ignore_whitespace: self.workspace.settings.ignore_whitespace,
            case_sensitive: self.workspace.settings.case_sensitive,
            replacement_rules,
            unimportant_section_rules,
            folder_includes: includes,
            folder_excludes: excludes,
            folder_display_filter: display,
            content_comparison: content,
            folder_alignment_overrides: match &self.workspace.current_view.view {
                DiffView::FolderCompare(folder) => {
                    folder.alignment_overrides.iter().map(Into::into).collect()
                }
                _ => vec![],
            },
            folder_column_widths: match &self.workspace.current_view.view {
                DiffView::FolderCompare(f) => f.column_widths.validated(),
                _ => Default::default(),
            },
            folder_sort: match &self.workspace.current_view.view {
                DiffView::FolderCompare(f) => crate::diff::settings::FolderSortStateV1 {
                    column: f.sort.column,
                    descending: f.sort.descending,
                },
                _ => Default::default(),
            },
            folder_compare_file_size: match &self.workspace.current_view.view {
                DiffView::FolderCompare(f) => f.compare_file_size,
                _ => true,
            },
            folder_compare_modified_timestamps: match &self.workspace.current_view.view {
                DiffView::FolderCompare(f) => f.compare_modified_timestamps,
                _ => true,
            },
            folder_timestamp_tolerance_seconds: match &self.workspace.current_view.view {
                DiffView::FolderCompare(f) => f.timestamp_tolerance.as_secs_f64(),
                _ => 2.0,
            },
            folder_use_text_compare_rules: match &self.workspace.current_view.view {
                DiffView::FolderCompare(f) => f.use_text_compare_rules,
                _ => true,
            },
            text_details_visible: text_model.is_some_and(|m| m.text_details_open),
            visible_whitespace: text_model.is_some_and(|m| m.visible_whitespace),
            sync_vertical: text_model.is_none_or(|m| m.scroll.sync_vertical),
            sync_horizontal: text_model.is_none_or(|m| m.scroll.sync_horizontal),
            projection_mode: text_model.map_or(0, |m| match m.projection_mode {
                crate::diff::text_compare::RowProjectionMode::All => 0,
                crate::diff::text_compare::RowProjectionMode::DifferencesOnly => 1,
                crate::diff::text_compare::RowProjectionMode::IdenticalOnly => 2,
            }),
            projection_context: text_model.map_or(3, |m| {
                m.projection_context
                    .min(crate::diff::text_compare::MAX_ROW_PROJECTION_CONTEXT)
                    as u8
            }),
        })
    }

    pub fn reopen_saved_session(
        &mut self,
        session: &crate::diff::persistence::SavedDiffSessionV1,
    ) -> Result<(), String> {
        self.ensure_memory_replacement_is_safe()?;
        let (left, right) =
            crate::diff::persistence::reopen_session(session).map_err(|e| e.to_string())?;
        self.workspace.settings.pane_split = session.pane_split;
        self.workspace.settings.wrap_text = session.wrap_text;
        self.workspace.settings.syntax_highlighting = session.syntax_highlighting;
        self.workspace.settings.syntax_theme = session.syntax_theme.clone();
        self.workspace.settings.ignore_whitespace = session.ignore_whitespace;
        self.workspace.settings.case_sensitive = session.case_sensitive;
        self.persistence.replacement_rules = session.replacement_rules.clone();
        self.persistence.unimportant_section_rules = session.unimportant_section_rules.clone();
        self.open_and_record(left, right)?;
        let view_id = self.workspace.current_view.id;
        self.restored_text_preferences.insert(
            view_id,
            (
                session.text_details_visible,
                session.visible_whitespace,
                session.sync_vertical,
                session.sync_horizontal,
                session.projection_mode,
                session.projection_context,
            ),
        );
        if let DiffView::FolderCompare(folder) = &mut self.workspace.current_view.view {
            folder.alignment_overrides = crate::diff::folder_compare::validate_alignment_overrides(
                &session
                    .folder_alignment_overrides
                    .iter()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| format!("invalid folder alignment override: {e}"))?;
            folder.draft_rules.include_rules = session.folder_includes.join("\n");
            folder.draft_rules.exclude_rules = session.folder_excludes.join("\n");
            folder.applied_scan_rules = crate::diff::folder_scan::ScanRules::validated(
                session.folder_includes.clone(),
                session.folder_excludes.clone(),
            )
            .map_err(|e| e.to_string())?;
            folder.display_filter = parse_folder_filter(&session.folder_display_filter);
            folder.content_comparison = match session.content_comparison {
                crate::diff::persistence::ContentComparisonModeV1::Metadata => {
                    crate::diff::model::ContentComparisonMode::Metadata
                }
                crate::diff::persistence::ContentComparisonModeV1::OnDemand => {
                    crate::diff::model::ContentComparisonMode::OnDemand
                }
                crate::diff::persistence::ContentComparisonModeV1::Always => {
                    crate::diff::model::ContentComparisonMode::Always
                }
            };
            folder.draft_rules.content_comparison = folder.content_comparison;
            folder.draft_rules.text_rules = folder.text_rules.clone();
            folder.draft_rules.timestamp_tolerance_seconds =
                session.folder_timestamp_tolerance_seconds.to_string();
            folder.timestamp_tolerance =
                std::time::Duration::from_secs_f64(session.folder_timestamp_tolerance_seconds);
            folder.compare_file_size = session.folder_compare_file_size;
            folder.compare_modified_timestamps = session.folder_compare_modified_timestamps;
            folder.use_text_compare_rules = session.folder_use_text_compare_rules;
            folder.draft_rules.compare_file_size = folder.compare_file_size;
            folder.draft_rules.compare_modified_timestamps = folder.compare_modified_timestamps;
            folder.draft_rules.use_text_compare_rules = folder.use_text_compare_rules;
            folder.column_widths = session.folder_column_widths.validated();
            folder.sort = crate::diff::model::FolderSortState {
                column: session.folder_sort.column,
                descending: session.folder_sort.descending,
            };
        }
        Ok(())
    }

    fn retained_view_ids(&self) -> HashSet<u64> {
        std::iter::once(self.workspace.current_view.id)
            .chain(self.workspace.navigation_stack.iter().map(|view| view.id))
            .collect()
    }

    fn reconcile_runtime_resources(&mut self) {
        let retained = self.retained_view_ids();
        let comparisons: HashMap<_, _> = std::iter::once(&self.workspace.current_view)
            .chain(&self.workspace.navigation_stack)
            .filter_map(|view| match &view.view {
                DiffView::FolderCompare(state) => {
                    Some((view.id, folder_comparison_identity(view.id, state)))
                }
                _ => None,
            })
            .collect();
        self.text_views.retain(|id, _| retained.contains(id));
        self.folder_runtimes.retain(|id, runtime| {
            comparisons
                .get(id)
                .is_some_and(|comparison| runtime.belongs_to(comparison))
        });
        self.binary_views.retain(|id, _| retained.contains(id));
        self.watch_views.retain(|id, _| retained.contains(id));
        if self
            .operation_preview
            .as_ref()
            .is_some_and(|preview| !self.operation_preview_is_current(preview))
        {
            self.operation_preview = None;
        }
    }

    fn clear_runtime_resources(&mut self) {
        self.text_views.clear();
        self.folder_runtimes.clear();
        self.binary_views.clear();
        self.watch_views.clear();
        self.operation_preview = None;
    }
    pub fn ui(&mut self, ctx: &egui::Context, activity_center: &crate::activity::ActivityCenter) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let screen = ctx.input(|i| i.content_rect());
        let chrome = window_chrome(ctx);
        let body_screen = inner_work_area(
            [screen.left(), screen.top(), screen.right(), screen.bottom()],
            chrome,
        );
        let (initial_size, initial_position) =
            crate::diff::model::validated_window_geometry(&self.persistence, body_screen);
        let minimum_size = crate::diff::model::diff_window_min_size(body_screen);
        let mut window = egui::Window::new("Diff")
            .id(egui::Id::new(("diff_window", self.workspace.workspace_id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(egui::Vec2::from(initial_size) + chrome)
            .min_size(egui::Vec2::from(minimum_size) + chrome);
        if let Some(position) = initial_position {
            window = window.default_pos(position);
        }
        let mut text_input_request = None;
        let response = window.show(ctx, |ui| {
            let body_viewport = DiffViewport::remaining(ui);
            allocate_viewport(ui, body_viewport, |ui| {
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)) {
                    self.navigate_back();
                }
                let header_height = ui.spacing().interact_size.y;
                let header_width = ui.available_width().max(0.0);
                egui::ScrollArea::horizontal()
                    .max_height(header_height)
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(header_width, header_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let path_width =
                                    header_path_width(header_width, ui.spacing().item_spacing.x)
                                        .min(header_width);
                                let left = ui.add_sized(
                                    [path_width, header_height],
                                    egui::TextEdit::singleline(&mut self.workspace.left_visible)
                                        .id(egui::Id::new((self.workspace.workspace_id, "left")))
                                        .hint_text("Left file or folder"),
                                );
                                if self.workspace.focus_left_requested {
                                    left.request_focus();
                                    self.workspace.focus_left_requested = false;
                                }
                                picker_menu(
                                    ui,
                                    &mut self.workspace,
                                    crate::diff::model::DiffSide::Left,
                                );
                                ui.label("↔");
                                ui.add_sized(
                                    [path_width, header_height],
                                    egui::TextEdit::singleline(&mut self.workspace.right_visible)
                                        .id(egui::Id::new((self.workspace.workspace_id, "right")))
                                        .hint_text("Right file or folder"),
                                );
                                picker_menu(
                                    ui,
                                    &mut self.workspace,
                                    crate::diff::model::DiffSide::Right,
                                );
                                if ui.button("Compare Text…").clicked() {
                                    text_input_request = Some(TextInputPurpose::NewComparison);
                                }
                                if matches!(
                                    &self.workspace.current_view.view,
                                    DiffView::TextCompare(state)
                                        if state.origin == TextCompareOrigin::Memory
                                ) && ui.button("Edit Inputs…").clicked()
                                {
                                    text_input_request = Some(TextInputPurpose::EditInputs {
                                        view_id: self.workspace.current_view.id,
                                    });
                                }
                                if ui
                                    .button("Compare")
                                    .on_hover_text("Compare/change paths (Ctrl+O)")
                                    .clicked()
                                    || ui.input_mut(|i| {
                                        i.consume_key(egui::Modifiers::CTRL, egui::Key::O)
                                    })
                                {
                                    let _ = self.open_and_record(
                                        self.workspace.left_visible.clone(),
                                        self.workspace.right_visible.clone(),
                                    );
                                }
                                ui.menu_button("More", |ui| {
                                    let active = !matches!(
                                        self.workspace.current_view.view,
                                        DiffView::Start
                                    );
                                    if ui
                                        .add_enabled(active, egui::Button::new("Swap Sides"))
                                        .clicked()
                                    {
                                        self.swap_sides();
                                        ui.close();
                                    }
                                    let active_comparison = matches!(
                                        self.workspace.current_view.view,
                                        DiffView::TextCompare(_) | DiffView::BinaryCompare(_)
                                    );
                                    let reloadable = matches!(
                                        &self.workspace.current_view.view,
                                        DiffView::TextCompare(state)
                                            if state.origin == TextCompareOrigin::FileBacked
                                    ) || matches!(
                                        self.workspace.current_view.view,
                                        DiffView::BinaryCompare(_)
                                    );
                                    if ui
                                        .add_enabled(reloadable, egui::Button::new("Reload Files"))
                                        .on_disabled_hover_text(
                                            "In-memory text has no files to reload",
                                        )
                                        .clicked()
                                    {
                                        self.reload_files();
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            active_comparison,
                                            egui::Button::new("Recompare"),
                                        )
                                        .clicked()
                                    {
                                        self.recompare();
                                        ui.close();
                                    }
                                    let folder = matches!(
                                        self.workspace.current_view.view,
                                        DiffView::FolderCompare(_)
                                    );
                                    if ui
                                        .add_enabled(folder, egui::Button::new("Refresh Folder"))
                                        .on_disabled_hover_text("Open a Folder comparison first")
                                        .clicked()
                                    {
                                        self.refresh_folder();
                                        ui.close();
                                    }
                                });
                            },
                        );
                    });
                if let Some(error) = &self.workspace.error {
                    bounded_message(ui, error, 48.0);
                }
                if !self.workspace.navigation_stack.is_empty() && ui.button("← Back").clicked() {
                    self.navigate_back();
                }
                ui.separator();
                let outcome = allocate_remaining_workspace(ui, |ui| {
                    self.render_workspace(ctx, ui, activity_center)
                })
                .inner;
                if let Some(error) = outcome.error {
                    self.workspace.error = Some(error);
                }
                if let Some(text_view::TextViewAction::CompareSelectionWithClipboard { view_id }) =
                    outcome.text_action
                    && let Err(error) = self.compare_selection_with_clipboard_using(view_id, || {
                        crate::actions::clipboard::get_text()
                            .map_err(|error| format!("Clipboard text is unavailable: {error}"))
                    })
                {
                    self.workspace.error = Some(error);
                }
                if let Some(recent) = outcome.recent_to_open {
                    match crate::diff::persistence::reopen_recent(&recent) {
                        Ok((left, right)) => {
                            let _ = self.open_and_record(left, right);
                        }
                        Err(error) => self.workspace.error = Some(error.to_string()),
                    }
                }
                self.apply_folder_action(outcome.folder_action);
                body_viewport.size
            })
            .inner
        });
        if let Some(purpose) = text_input_request {
            self.begin_text_input(purpose);
        }
        self.show_text_input(ctx);
        self.show_operation_preview(ctx);
        if let Some(response) = response
            && let Some(inner_size) = response.inner
            && let Some((size, position)) =
                runtime_window_geometry(inner_size, response.response.rect.left_top(), body_screen)
        {
            self.persistence.window_size = Some(size);
            self.persistence.window_position = Some(position);
        }
        if !open && self.text_views.values().any(|m| m.has_dirty()) {
            self.close_prompt = true;
            open = true;
        }
        if self.close_prompt {
            egui::Window::new("Unsaved comparison")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("One or both sides contain unsaved changes.");
                    ui.horizontal(|ui| {
                        if ui.button("Save modified").clicked() {
                            let ok = self.text_views.values_mut().all(|m| {
                                let mut ok = true;
                                if m.left.is_dirty() {
                                    ok &=
                                        text_view::save_side(m, crate::diff::model::DiffSide::Left)
                                }
                                if m.right.is_dirty() {
                                    ok &=
                                        text_view::save_side(m, crate::diff::model::DiffSide::Right)
                                }
                                ok
                            });
                            if ok {
                                self.close_prompt = false;
                                open = false;
                            }
                        }
                        if ui.button("Discard").clicked() {
                            self.close_prompt = false;
                            open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.cancel_close_prompt(&mut open);
                        }
                    });
                });
        }
        self.open = open;
        if !self.open {
            self.clear_runtime_resources();
        }
    }

    /// Renders the retained active view and defers workspace mutations until its
    /// mutable borrow has ended in the caller.
    fn render_workspace(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        activity_center: &crate::activity::ActivityCenter,
    ) -> WorkspaceRenderOutcome {
        // Copy only lightweight context before borrowing the retained view.
        let workspace_id = self.workspace.workspace_id;
        let view_id = self.workspace.current_view.id;
        let settings = self.workspace.settings.clone();
        let mut action = folder_view::FolderViewAction::Noop;
        let mut text_action = None;
        let mut render_error = None;
        let mut recent_to_open = None;
        match &mut self.workspace.current_view.view {
            DiffView::Start => {
                ui.label("Choose two files or two folders to compare.");
                if !self.persistence.recent_comparisons.is_empty() {
                    ui.heading("Recent comparisons");
                    let recents = self.persistence.recent_comparisons.clone();
                    for recent in recents {
                        let label =
                            format!("{} ↔ {} ({:?})", recent.left, recent.right, recent.mode);
                        if ui.button(label).clicked() {
                            recent_to_open = Some(recent);
                        }
                    }
                }
            }
            DiffView::TextCompare(s) => {
                if !self.text_views.contains_key(&view_id) {
                    match crate::diff::model::TextViewModel::load(s, &settings) {
                        Ok(mut m) => {
                            configure_text_model(&mut m, &settings, &self.persistence);
                            if let Some(preferences) =
                                self.restored_text_preferences.remove(&view_id)
                            {
                                apply_restored_text_preferences(&mut m, preferences);
                            }
                            self.text_views.insert(view_id, m);
                        }
                        Err(e) => {
                            render_error = Some(e);
                        }
                    }
                }
                if s.origin == TextCompareOrigin::Memory {
                    self.watch_views.remove(&view_id);
                } else {
                    let tag = crate::diff::watch::WatchTag {
                        workspace: workspace_id,
                        view: view_id,
                        generation: 1,
                    };
                    if self
                        .watch_views
                        .get(&view_id)
                        .is_none_or(|watch| watch.tag != tag)
                    {
                        self.watch_views.insert(
                            view_id,
                            crate::diff::watch::ViewWatchRuntime::text(
                                tag,
                                s.left.clone(),
                                s.right.clone(),
                            ),
                        );
                    }
                }
                if let Some(m) = self.text_views.get_mut(&view_id) {
                    let dirty = [m.left.is_dirty(), m.right.is_dirty()];
                    let actions = if s.origin == TextCompareOrigin::Memory {
                        Vec::new()
                    } else {
                        self.watch_views
                            .get_mut(&view_id)
                            .map(|w| w.poll(std::time::Instant::now(), dirty))
                            .unwrap_or_default()
                    };
                    for action in actions {
                        match action {
                            crate::diff::watch::ViewWatchAction::TextReload { side, loaded } => {
                                if let Err(e) = m.reload_external(side, &loaded) {
                                    render_error = Some(e);
                                }
                            }
                            crate::diff::watch::ViewWatchAction::TextConflict { side, .. } => {
                                m.external_conflict[if side == crate::diff::model::DiffSide::Left {
                                    0
                                } else {
                                    1
                                }] = true
                            }
                            _ => {}
                        }
                    }
                    for (index, side, label) in [
                        (0, crate::diff::model::DiffSide::Left, "Left"),
                        (1, crate::diff::model::DiffSide::Right, "Right"),
                    ] {
                        if m.external_conflict[index] {
                            ui.horizontal_wrapped(|ui| {
                                let message = format!(
                                    "{label} changed on disk; in-memory edits were preserved."
                                );
                                semantic_status(ui, StatusTone::Warning, &message, None);
                                if ui.button("Reload / discard edits").clicked()
                                    && let Some(loaded) = self
                                        .watch_views
                                        .get_mut(&view_id)
                                        .and_then(|w| w.resolve_text_conflict(side, true))
                                {
                                    let _ = m.reload_external(side, &loaded);
                                }
                                if ui.button("Keep current").clicked() {
                                    if let Some(w) = self.watch_views.get_mut(&view_id) {
                                        w.resolve_text_conflict(side, false);
                                    }
                                    m.external_conflict[index] = false;
                                }
                            });
                        }
                    }
                    text_action = text_view::show(ui, workspace_id, view_id, m);
                }
                let source_label = |path: Option<&std::path::PathBuf>| {
                    if s.origin == TextCompareOrigin::Memory {
                        "In-memory text".to_string()
                    } else {
                        path.map_or_else(
                            || "(missing folder side)".into(),
                            |path| path.display().to_string(),
                        )
                    }
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Left: {}", source_label(s.left.as_ref())));
                    ui.label(format!("Right: {}", source_label(s.right.as_ref())));
                });
            }
            DiffView::BinaryCompare(s) => {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    self.binary_views.entry(view_id)
                {
                    match crate::diff::binary_compare::BinaryViewModel::load(s, settings.pane_split)
                    {
                        Ok(model) => {
                            entry.insert(model);
                        }
                        Err(error) => {
                            render_error = Some(error);
                        }
                    }
                }
                let tag = crate::diff::watch::WatchTag {
                    workspace: workspace_id,
                    view: view_id,
                    generation: 1,
                };
                if self
                    .watch_views
                    .get(&view_id)
                    .is_none_or(|watch| watch.tag != tag)
                {
                    self.watch_views.insert(
                        view_id,
                        crate::diff::watch::ViewWatchRuntime::binary(
                            tag,
                            s.left.clone(),
                            s.right.clone(),
                        ),
                    );
                }
                let refresh = self.watch_views.get_mut(&view_id).is_some_and(|watch| {
                    watch
                        .poll(std::time::Instant::now(), [false; 2])
                        .into_iter()
                        .any(|a| matches!(a, crate::diff::watch::ViewWatchAction::BinaryRefresh))
                });
                if refresh
                    && let Some(model) = self.binary_views.get_mut(&view_id)
                    && let Err(error) = model.refresh_external(s)
                {
                    render_error = Some(format!("Binary view is stale: {error}"));
                }
                if let Some(model) = self.binary_views.get_mut(&view_id) {
                    let _ = binary_view::show(ui, workspace_id, view_id, model);
                }
            }
            DiffView::FolderCompare(s) => {
                let runtime = self.folder_runtimes.entry(view_id).or_default();
                runtime.attach_activity_center(activity_center.clone());
                poll_folder_runtime(view_id, s, runtime);
                let tag = crate::diff::watch::WatchTag {
                    workspace: workspace_id,
                    view: view_id,
                    generation: runtime.generation,
                };
                if self
                    .watch_views
                    .get(&view_id)
                    .is_none_or(|watch| watch.tag != tag)
                {
                    self.watch_views.insert(
                        view_id,
                        crate::diff::watch::ViewWatchRuntime::folder(
                            tag,
                            s.left_root.clone(),
                            s.right_root.clone(),
                        ),
                    );
                }
                let changes = self
                    .watch_views
                    .get_mut(&view_id)
                    .map(|w| w.poll(std::time::Instant::now(), [false; 2]))
                    .unwrap_or_default();
                for change in changes {
                    if let crate::diff::watch::ViewWatchAction::FolderChanged { subtree, .. } =
                        change
                    {
                        for entry in s.model.entries.values() {
                            if entry.relative_path.starts_with(&subtree) {
                                s.stale_paths.insert(entry.relative_path.clone());
                            }
                        }
                        // A bounded replacement scan discovers additions/removals; it
                        // never performs synchronization or filesystem mutation.
                        runtime.prepare_rescan();
                    }
                }
                if runtime.is_active() {
                    ctx.request_repaint();
                }
                if runtime.left_error.is_some() || runtime.right_error.is_some() {
                    let left = runtime.left_error.as_deref().unwrap_or("none");
                    let right = runtime.right_error.as_deref().unwrap_or("none");
                    bounded_message(
                        ui,
                        &format!("Scan failures — left: {left}; right: {right}"),
                        48.0,
                    );
                }
                if let Some(feedback) = &runtime.operation_feedback {
                    show_folder_operation_feedback(ui, view_id, feedback);
                }
                action = render_retained_folder(s, |state| folder_view::show(ui, state, runtime));
                let sort = crate::diff::settings::FolderSortStateV1 {
                    column: s.sort.column,
                    descending: s.sort.descending,
                };
                self.workspace.settings.folder_sort = sort;
                self.workspace.settings.folder_column_widths = s.column_widths.validated();
                self.persistence.config.folder_sort = sort;
                self.persistence.config.folder_column_widths = s.column_widths.validated();
            }
        }
        WorkspaceRenderOutcome {
            folder_action: action,
            text_action,
            error: render_error,
            recent_to_open,
        }
    }

    fn apply_folder_action(&mut self, action: folder_view::FolderViewAction) {
        match action {
            folder_view::FolderViewAction::Noop => {}
            folder_view::FolderViewAction::OpenChild {
                relative_path,
                left,
                right,
            } => {
                if let Err(error) = self.workspace.push_file_compare(relative_path, left, right) {
                    self.workspace.error = Some(error);
                }
            }
            folder_view::FolderViewAction::NavigateBack => {
                self.navigate_back();
            }
            folder_view::FolderViewAction::RequestRescan => {
                if let DiffView::FolderCompare(state) = &mut self.workspace.current_view.view
                    && state.apply_draft().is_ok()
                {
                    state.model = Default::default();
                    state.left_scan_complete = false;
                    state.right_scan_complete = false;
                    state.stale_paths.clear();
                    // Keep view controls and selection. Non-surviving paths are
                    // naturally harmless until the replacement model arrives.
                    self.folder_runtimes
                        .entry(self.workspace.current_view.id)
                        .or_default()
                        .prepare_rescan();
                }
            }
            folder_view::FolderViewAction::RequestMutation(kind) => self.plan_mutation(kind),
        }
        self.reconcile_runtime_resources();
    }

    fn plan_mutation(&mut self, kind: folder_view::MutationKind) {
        use crate::diff::file_ops::{CopyDirection, DeleteMode};
        let view_id = self.workspace.current_view.id;
        let Some(runtime) = self.folder_runtimes.get(&view_id) else {
            return;
        };
        if runtime.mutation_active() {
            return;
        }
        let DiffView::FolderCompare(state) = &self.workspace.current_view.view else {
            return;
        };
        // Capture once. Filtering also ensures a command is never routed for a
        // selection which exists only on the other side.
        let mut captured = folder_view::selection_snapshot(state);
        let source_is_left = matches!(
            kind,
            folder_view::MutationKind::CopyRight | folder_view::MutationKind::DeleteLeft
        );
        captured.retain(|path| {
            state.model.entries.values().any(|entry| {
                &entry.relative_path == path
                    && if source_is_left {
                        entry.left.is_some()
                    } else {
                        entry.right.is_some()
                    }
            })
        });
        if captured.is_empty() {
            return;
        }
        let generation = runtime.generation;
        let planned = match kind {
            folder_view::MutationKind::CopyRight => crate::diff::file_ops::plan_copy(
                &state.left_root,
                &state.right_root,
                CopyDirection::LeftToRight,
                captured,
                generation,
            )
            .map(operation_preview::PreviewPlan::Copy),
            folder_view::MutationKind::CopyLeft => crate::diff::file_ops::plan_copy(
                &state.right_root,
                &state.left_root,
                CopyDirection::RightToLeft,
                captured,
                generation,
            )
            .map(operation_preview::PreviewPlan::Copy),
            folder_view::MutationKind::DeleteLeft => crate::diff::file_ops::plan_delete(
                &state.left_root,
                "Left",
                captured,
                generation,
                DeleteMode::Recycle,
            )
            .map(operation_preview::PreviewPlan::Delete),
            folder_view::MutationKind::DeleteRight => crate::diff::file_ops::plan_delete(
                &state.right_root,
                "Right",
                captured,
                generation,
                DeleteMode::Recycle,
            )
            .map(operation_preview::PreviewPlan::Delete),
        };
        let dirty = self.dirty_paths();
        match planned {
            Ok(plan) => {
                let conflicts: Vec<_> = match &plan {
                    operation_preview::PreviewPlan::Copy(copy) => copy
                        .copies
                        .iter()
                        .filter(|item| {
                            crate::diff::file_ops::mutation_contains_dirty_path(
                                &item.target,
                                &dirty,
                            )
                        })
                        .map(|item| item.relative.display().to_string())
                        .collect(),
                    operation_preview::PreviewPlan::Delete(delete) => delete
                        .items
                        .iter()
                        .filter(|item| {
                            crate::diff::file_ops::mutation_contains_dirty_path(
                                &item.target,
                                &dirty,
                            )
                        })
                        .map(|item| item.relative.display().to_string())
                        .collect(),
                };
                self.operation_preview = Some(operation_preview::OperationPreview {
                    context: FolderOperationContext {
                        comparison: folder_comparison_identity(view_id, state),
                        generation,
                    },
                    plan,
                    planning_error: (!conflicts.is_empty()).then(|| {
                        format!(
                            "Blocked: unsaved Diff changes would be overwritten or deleted at {}",
                            conflicts.join(", ")
                        )
                    }),
                })
            }
            Err(error) => {
                self.workspace.error = Some(format!("Cannot plan folder operation: {error}"))
            }
        }
    }

    fn dirty_paths(&self) -> HashSet<std::path::PathBuf> {
        let mut paths = HashSet::new();
        for model in self.text_views.values() {
            if model.left.is_dirty()
                && let Some(path) = &model.left_path
            {
                paths.insert(path.clone());
            }
            if model.right.is_dirty()
                && let Some(path) = &model.right_path
            {
                paths.insert(path.clone());
            }
        }
        paths
    }

    fn show_operation_preview(&mut self, ctx: &egui::Context) {
        if self
            .operation_preview
            .as_ref()
            .is_some_and(|preview| !self.operation_preview_is_current(preview))
        {
            self.operation_preview = None;
        }
        let active = self
            .operation_preview
            .as_ref()
            .and_then(|p| self.folder_runtimes.get(&p.context.comparison.view_id))
            .is_some_and(|runtime| runtime.mutation_active());
        let response = self.operation_preview.as_mut().map(|p| p.show(ctx, active));
        if let Some(response) = response {
            self.apply_operation_preview_response(response);
        }
    }

    fn operation_preview_is_current(&self, preview: &operation_preview::OperationPreview) -> bool {
        let view_id = self.workspace.current_view.id;
        let DiffView::FolderCompare(state) = &self.workspace.current_view.view else {
            return false;
        };
        let comparison = folder_comparison_identity(view_id, state);
        preview.context.comparison == comparison
            && self.folder_runtimes.get(&view_id).is_some_and(|runtime| {
                runtime.generation == preview.context.generation && runtime.belongs_to(&comparison)
            })
    }

    fn apply_operation_preview_response(&mut self, response: operation_preview::PreviewResponse) {
        match response {
            operation_preview::PreviewResponse::Cancel => self.operation_preview = None,
            operation_preview::PreviewResponse::Execute => {
                let Some(preview) = self.operation_preview.take() else {
                    return;
                };
                if !self.operation_preview_is_current(&preview)
                    || preview.planning_error.is_some()
                    || self
                        .folder_runtimes
                        .get(&preview.context.comparison.view_id)
                        .is_some_and(|runtime| runtime.mutation_active())
                {
                    return;
                }
                let dirty = self.dirty_paths();
                let handle = match preview.plan {
                    operation_preview::PreviewPlan::Copy(plan) => {
                        Ok(crate::diff::file_ops::spawn_copy(plan, dirty))
                    }
                    operation_preview::PreviewPlan::Delete(plan) => {
                        crate::diff::file_ops::spawn_recycle_delete(
                            plan,
                            dirty,
                            std::sync::Arc::new(crate::diff::file_ops::SystemTrash),
                        )
                    }
                };
                match handle {
                    Ok(handle) => {
                        if let Some(runtime) = self
                            .folder_runtimes
                            .get_mut(&preview.context.comparison.view_id)
                        {
                            runtime.start_operation(preview.context, handle);
                        }
                    }
                    Err(error) => self.workspace.error = Some(error),
                }
            }
            _ => {}
        }
    }

    /// The single Back transition used by both keyboard and button activation.
    fn navigate_back(&mut self) -> bool {
        let changed_relative = match &self.workspace.current_view.view {
            DiffView::TextCompare(state) => self
                .text_views
                .get(&self.workspace.current_view.id)
                .filter(|model| model.saved_filesystem_mutation)
                .and_then(|_| state.relative_path.clone()),
            _ => None,
        };
        let moved = self.workspace.back();
        if moved {
            if let (Some(relative), DiffView::FolderCompare(folder)) =
                (changed_relative, &mut self.workspace.current_view.view)
            {
                folder.stale_paths.insert(relative);
            }
            self.reconcile_runtime_resources();
        }
        moved
    }
}

/// Leaves a conservative fixed budget for two Browse menus, the arrow, Compare,
/// and inter-widget spacing, then shares all flexible width between path editors.
fn header_path_width(available_width: f32, spacing: f32) -> f32 {
    ((available_width.max(0.0) - 230.0 - spacing.max(0.0) * 6.0) / 2.0).max(24.0)
}

fn folder_filter_name(value: &crate::diff::model::FolderDisplayFilter) -> &'static str {
    use crate::diff::model::FolderDisplayFilter::*;
    match value {
        All => "all",
        Differences => "differences",
        Identical => "identical",
        LeftOnly => "left_only",
        RightOnly => "right_only",
        LeftNewer => "left_newer",
        RightNewer => "right_newer",
        Errors => "errors",
        LeftChanges => "left_changes",
        RightChanges => "right_changes",
        Orphans => "orphans",
        Changes => "changes",
        Combined(_) => "combined",
    }
}

fn parse_folder_filter(value: &str) -> crate::diff::model::FolderDisplayFilter {
    use crate::diff::model::FolderDisplayFilter::*;
    match value {
        "differences" => Differences,
        "identical" => Identical,
        "left_only" => LeftOnly,
        "right_only" => RightOnly,
        "left_newer" => LeftNewer,
        "right_newer" => RightNewer,
        "errors" => Errors,
        "left_changes" => LeftChanges,
        "right_changes" => RightChanges,
        "orphans" => Orphans,
        "changes" => Changes,
        _ => All,
    }
}

fn folder_comparison_identity(
    view_id: u64,
    state: &crate::diff::model::FolderCompareState,
) -> FolderComparisonIdentity {
    FolderComparisonIdentity {
        view_id,
        left_root: crate::diff::folder_scan::RootIdentity::new(state.left_root.clone()),
        right_root: crate::diff::folder_scan::RootIdentity::new(state.right_root.clone()),
    }
}

fn folder_operation_summary(feedback: &FolderOperationFeedback) -> String {
    format!(
        "Folder operation{}: {} completed, {} failed.",
        if feedback.cancelled { " cancelled" } else { "" },
        feedback.completed,
        feedback.failed,
    )
}

fn show_folder_operation_feedback(
    ui: &mut egui::Ui,
    view_id: u64,
    feedback: &FolderOperationFeedback,
) {
    let tone = if feedback.failed > 0 {
        StatusTone::Error
    } else if feedback.cancelled {
        StatusTone::Warning
    } else {
        StatusTone::Success
    };
    ui.horizontal_wrapped(|ui| {
        semantic_status(ui, tone, &folder_operation_summary(feedback), None);
    });
    if !feedback.failures.is_empty() {
        egui::CollapsingHeader::new("Operation failures")
            .id_salt(("folder-operation-failures", view_id))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for failure in &feedback.failures {
                            ui.label(format!(
                                "{}: {}",
                                failure.relative.display(),
                                failure.message
                            ));
                        }
                        if feedback.omitted_failures > 0 {
                            ui.label(format!(
                                "{} additional failure details omitted.",
                                feedback.omitted_failures
                            ));
                        }
                    });
            });
    }
}

/// Production and tests use the same adoption boundary. Validate the receiving
/// comparison before accepting feedback or requesting a completion rescan.
fn process_operation_events(
    view_id: u64,
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
    context: &FolderOperationContext,
    events: impl IntoIterator<Item = crate::diff::file_ops::OperationEvent>,
) {
    let comparison = folder_comparison_identity(view_id, state);
    runtime.reconcile_comparison(comparison.clone());
    if context.comparison != comparison {
        return;
    }
    for event in events {
        if let crate::diff::file_ops::OperationEvent::Completed(report) = event
            && runtime.finish_operation(context, &report)
        {
            refresh_after_mutation(state, runtime, report.affected_subtree);
            break;
        }
    }
}

fn poll_folder_runtime(
    view_id: u64,
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
) {
    let comparison = folder_comparison_identity(view_id, state);
    runtime.reconcile_comparison(comparison.clone());
    if let Some((context, events)) = runtime.operation_events() {
        process_operation_events(view_id, state, runtime, &context, events);
    }

    // A child editor can invalidate one row. Refresh it directly instead of
    // discarding the retained folder model or restarting both root scans.
    for relative in std::mem::take(&mut state.stale_paths) {
        if let Some(entry) = state
            .model
            .entries
            .values_mut()
            .find(|entry| entry.relative_path == relative)
        {
            for side in [&mut entry.left, &mut entry.right].into_iter().flatten() {
                match std::fs::metadata(&side.path) {
                    Ok(metadata) => {
                        let kind = side
                            .metadata
                            .as_ref()
                            .map_or(crate::diff::folder_compare::EntryKind::File, |metadata| {
                                metadata.kind
                            });
                        side.metadata = Some(crate::diff::folder_compare::EntryMetadata::from_fs(
                            &metadata, kind,
                        ));
                        side.error = None;
                    }
                    Err(error) => side.error = Some(error.to_string()),
                }
            }
            entry.metadata_status = crate::diff::folder_compare::fast_status(
                entry.left.as_ref(),
                entry.right.as_ref(),
                state.timestamp_tolerance,
            );
            entry.effective_status = entry.metadata_status;
            entry.content_checked = false;
            state.model.revision = state.model.revision.wrapping_add(1);
        }
    }

    let left_identity = comparison.left_root;
    let right_identity = comparison.right_root;
    if runtime.left_root.as_ref() != Some(&left_identity)
        || runtime.right_root.as_ref() != Some(&right_identity)
    {
        runtime.cancel();
        if runtime.restart_prepared {
            runtime.restart_prepared = false;
        } else {
            runtime.generation = crate::diff::folder_runtime::FolderRuntime::next_generation();
        }
        runtime.left_root = Some(left_identity.clone());
        runtime.right_root = Some(right_identity.clone());
        runtime.left_visited = 0;
        runtime.right_visited = 0;
        runtime.comparison_queue.clear();
        runtime.completed_comparisons = 0;
        runtime.left_error = None;
        runtime.right_error = None;
        state.left_scan_complete = false;
        state.right_scan_complete = false;
        runtime.left_scan = Some(runtime.spawn_scan(
            state.left_root.clone(),
            runtime.generation,
            state.applied_scan_rules.clone(),
            crate::activity::ActivitySlot::Primary,
        ));
        runtime.right_scan = Some(runtime.spawn_scan(
            state.right_root.clone(),
            runtime.generation,
            state.applied_scan_rules.clone(),
            crate::activity::ActivitySlot::Secondary,
        ));
    }

    let left_events: Vec<_> = runtime
        .left_scan
        .as_ref()
        .map(|h| h.receiver.try_iter().collect())
        .unwrap_or_default();
    let right_events: Vec<_> = runtime
        .right_scan
        .as_ref()
        .map(|h| h.receiver.try_iter().collect())
        .unwrap_or_default();
    process_scan_events(state, runtime, true, &left_identity, left_events);
    process_scan_events(state, runtime, false, &right_identity, right_events);

    if state.left_scan_complete && state.right_scan_complete {
        let surviving: HashSet<_> = state
            .model
            .entries
            .values()
            .map(|entry| entry.relative_path.clone())
            .collect();
        state.selected_paths.retain(|path| surviving.contains(path));
        if state
            .primary_selection
            .as_ref()
            .is_some_and(|path| !surviving.contains(path))
        {
            state.primary_selection = state.selected_paths.iter().next().cloned();
        }
        if state
            .scroll_anchor
            .as_ref()
            .is_some_and(|path| !surviving.contains(path))
        {
            state.scroll_anchor = state.primary_selection.clone();
        }
    }

    let visible = folder_view::ordered_visible(state);
    runtime.prioritize(state.primary_selection.as_ref(), &visible, &state.model);
    runtime.pump(&mut state.model, &state.text_rules);
}

/// Deterministic controller seam: production passes events drained from scan
/// handles; tests inject the same event stream without threads or egui.
fn process_scan_events(
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
    left: bool,
    expected: &crate::diff::folder_scan::RootIdentity,
    events: impl IntoIterator<Item = crate::diff::folder_scan::ScanEvent>,
) {
    use crate::diff::folder_compare::PathKeyPolicy;
    use crate::diff::folder_scan::ScanEvent;
    for event in events {
        if !event.is_current(runtime.generation, expected) {
            continue;
        }
        match event {
            ScanEvent::ScanStarted { .. } => {}
            ScanEvent::EntriesDiscovered { entries, .. }
            | ScanEvent::EntriesUpdated { entries, .. } => {
                for item in entries {
                    let _ = state.model.upsert(
                        &item.relative_path,
                        item.side,
                        left,
                        PathKeyPolicy::Platform,
                        state.timestamp_tolerance,
                    );
                }
            }
            ScanEvent::Progress { visited, .. } => {
                if left {
                    runtime.left_visited = visited
                } else {
                    runtime.right_visited = visited
                }
            }
            ScanEvent::Completed { visited, .. } | ScanEvent::Cancelled { visited, .. } => {
                if left {
                    runtime.left_visited = visited;
                    state.left_scan_complete = true;
                    runtime.left_scan = None;
                } else {
                    runtime.right_visited = visited;
                    state.right_scan_complete = true;
                    runtime.right_scan = None;
                }
            }
            ScanEvent::Failed { error, .. } => {
                if left {
                    runtime.left_error = Some(error);
                    state.left_scan_complete = true;
                    runtime.left_scan = None;
                } else {
                    runtime.right_error = Some(error);
                    state.right_scan_complete = true;
                    runtime.right_scan = None;
                }
            }
        }
    }
}

/// Refresh boundary intentionally accepts the affected subtree even though the
/// first implementation performs a generation-safe full comparison scan.
fn refresh_after_mutation(
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
    _affected_subtree: Option<std::path::PathBuf>,
) {
    state.model = Default::default();
    state.left_scan_complete = false;
    state.right_scan_complete = false;
    state.stale_paths.clear();
    // selected_paths, primary_selection, and scroll_anchor deliberately
    // survive until completed scans reconcile them against surviving rows.
    runtime.prepare_rescan();
}

fn picker_menu(
    ui: &mut egui::Ui,
    workspace: &mut DiffWorkspace,
    side: crate::diff::model::DiffSide,
) {
    ui.menu_button("Browse ▾", |ui| {
        if ui.button("Select File…").clicked() {
            workspace.assign_selected_path(side, rfd::FileDialog::new().pick_file());
            ui.close();
        }
        if ui.button("Select Folder…").clicked() {
            workspace.assign_selected_path(side, rfd::FileDialog::new().pick_folder());
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::file_ops::{ExecutionReport, ItemOutcome, ItemResult, OperationEvent};
    use crate::diff::model::{DiffView, FolderCompareState, RetainedView};

    fn dirty_memory_dialog() -> (DiffDialogState, u64) {
        let mut dialog = DiffDialogState {
            open: true,
            ..Default::default()
        };
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "initial left".into();
        draft.right = "initial right".into();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let view_id = dialog.workspace.current_view.id;
        dialog
            .text_views
            .get_mut(&view_id)
            .unwrap()
            .left
            .replace_source("edited scratch".into())
            .unwrap();
        (dialog, view_id)
    }

    fn memory_dialog_with_inputs(left: &str, right: &str) -> (DiffDialogState, u64) {
        let mut dialog = DiffDialogState::default();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = left.to_owned();
        draft.right = right.to_owned();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let view_id = dialog.workspace.current_view.id;
        (dialog, view_id)
    }

    fn folder_dialog(id: u64) -> DiffDialogState {
        let mut dialog = DiffDialogState::default();
        dialog.workspace.current_view = RetainedView {
            id,
            view: DiffView::FolderCompare(Box::default()),
        };
        dialog
    }

    #[test]
    fn geometry_is_independent_of_view_metadata_and_latest_update_wins() {
        let mut dialog = folder_dialog(7);
        dialog.persistence.window_size = Some([600.0, 350.0]);
        dialog.persistence.window_position = Some([12.0, 24.0]);
        let before = crate::diff::model::validated_window_geometry(
            &dialog.persistence,
            [0.0, 0.0, 1600.0, 1000.0],
        );
        dialog.workspace.current_view.view = DiffView::Start;
        assert_eq!(
            before,
            crate::diff::model::validated_window_geometry(
                &dialog.persistence,
                [0.0, 0.0, 1600.0, 1000.0]
            )
        );
        dialog.persistence.window_size = Some([777.0, 555.0]);
        dialog.persistence.window_size = Some([888.0, 666.0]);
        assert_eq!(
            crate::diff::model::validated_window_geometry(
                &dialog.persistence,
                [0.0, 0.0, 1600.0, 1000.0]
            )
            .0,
            [888.0, 666.0]
        );
    }

    #[test]
    fn runtime_geometry_accepts_valid_and_effective_minimum_rectangles() {
        let screen = [0.0, 0.0, 1920.0, 1080.0];
        assert_eq!(
            runtime_window_geometry(egui::vec2(1000.0, 700.0), egui::pos2(12.0, 24.0), screen),
            Some(([1000.0, 700.0], [12.0, 24.0]))
        );
        assert_eq!(
            runtime_window_geometry(egui::vec2(400.0, 250.0), egui::pos2(3.0, 4.0), screen),
            Some((crate::diff::model::DIFF_MIN_SIZE, [3.0, 4.0]))
        );
        assert_eq!(
            runtime_window_geometry(
                egui::vec2(300.0, 200.0),
                egui::pos2(7.0, 9.0),
                [0.0, 0.0, 300.0, 200.0]
            ),
            Some(([300.0, 200.0], [7.0, 9.0]))
        );
    }

    #[test]
    fn runtime_geometry_accepts_attainable_body_on_small_work_area() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let chrome = window_chrome(ui.ctx());
            let screen = inner_work_area([0.0, 0.0, 300.0, 200.0], chrome);
            let size = egui::vec2(screen[2], screen[3]);
            assert!(size.x < 300.0 && size.y < 200.0);
            assert_eq!(
                runtime_window_geometry(size, egui::Pos2::ZERO, screen),
                Some(([size.x, size.y], [0.0, 0.0]))
            );
            assert!(
                runtime_window_geometry(size - egui::Vec2::splat(1.0), egui::Pos2::ZERO, screen)
                    .is_none()
            );
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn runtime_geometry_rejects_incomplete_rectangles_without_partial_persistence() {
        let screen = [0.0, 0.0, 1920.0, 1080.0];
        let rejected = [
            (egui::vec2(f32::NAN, 650.0), egui::pos2(20.0, 30.0)),
            (egui::vec2(f32::INFINITY, 650.0), egui::pos2(20.0, 30.0)),
            (egui::vec2(399.0, 650.0), egui::pos2(20.0, 30.0)),
            (egui::vec2(900.0, 249.0), egui::pos2(20.0, 30.0)),
            (egui::vec2(900.0, 650.0), egui::pos2(f32::NAN, 30.0)),
            (egui::vec2(900.0, 650.0), egui::pos2(20.0, f32::INFINITY)),
        ];
        let mut persistence = crate::diff::persistence::DiffPersistenceV1::default();
        persistence.window_size = Some([1000.0, 700.0]);
        persistence.window_position = Some([20.0, 30.0]);
        for (size, position) in rejected {
            if let Some((size, position)) = runtime_window_geometry(size, position, screen) {
                persistence.window_size = Some(size);
                persistence.window_position = Some(position);
            }
            assert_eq!(persistence.window_size, Some([1000.0, 700.0]));
            assert_eq!(persistence.window_position, Some([20.0, 30.0]));
        }
    }

    #[test]
    fn repeated_open_persists_the_bounded_inner_viewport() {
        for (font_override, extra_line_spacing) in [(None, 0.0), (Some(32.0), 0.0), (None, 3.5)] {
            let ctx = egui::Context::default();
            ctx.style_mut_of(ctx.theme(), |style| {
                style.override_font_id = font_override.map(egui::FontId::proportional);
                style.spacing.extra_text_line_spacing = extra_line_spacing;
            });
            let screen = [0.0, 0.0, 1600.0, 1200.0];
            let mut persistence = crate::diff::persistence::DiffPersistenceV1::default();
            persistence.window_size = Some([900.0, 650.0]);
            persistence.window_position = Some([40.0, 50.0]);
            let tolerance = 0.01;

            for workspace_id in 100..103 {
                let (restored_size, restored_position) =
                    crate::diff::model::validated_window_geometry(&persistence, screen);
                assert!((restored_size[0] - 900.0).abs() <= tolerance);
                assert!((restored_size[1] - 650.0).abs() <= tolerance);

                let mut observation = None;
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_max(
                            egui::pos2(screen[0], screen[1]),
                            egui::pos2(screen[2], screen[3]),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        let chrome = window_chrome(ctx.ctx());
                        let mut open = true;
                        let mut window = egui::Window::new("Diff")
                            .id(egui::Id::new(("geometry_regression", workspace_id)))
                            .open(&mut open)
                            .collapsible(false)
                            .resizable(true)
                            .default_size(egui::Vec2::from(restored_size) + chrome)
                            .min_size(
                                egui::Vec2::from(crate::diff::model::diff_window_min_size(
                                    inner_work_area(screen, chrome),
                                )) + chrome,
                            );
                        if let Some(position) = restored_position {
                            window = window.default_pos(position);
                        }
                        let response = window
                            .show(ctx, |ui| {
                                let viewport = DiffViewport::remaining(ui);
                                allocate_viewport(ui, viewport, |_| viewport.size).inner
                            })
                            .unwrap();
                        observation = Some((response.inner, response.response.rect, chrome));
                    },
                )
                .drop_without_applying_deltas();

                let (inner_size, outer_rect, chrome) = observation.unwrap();
                let inner_size = inner_size.expect("window body rendered");
                let geometry = runtime_window_geometry(
                    inner_size,
                    outer_rect.left_top(),
                    inner_work_area(screen, chrome),
                )
                .expect("complete window geometry");
                assert!((geometry.0[0] - 900.0).abs() <= tolerance);
                assert!(
                    (geometry.0[1] - 650.0).abs() <= tolerance,
                    "workspace={workspace_id} override={font_override:?} restored={restored_size:?} chrome={chrome:?} body={inner_size:?} outer={outer_rect:?} actual_chrome={:?}",
                    outer_rect.size() - inner_size
                );
                // The Area response is permitted to carry frame/title-bar size.
                // Model that explicitly rather than depending on platform styling
                // (some egui test styles make the response and body equally sized),
                // and prove that only its position participates in persistence.
                let outer_size_with_chrome = outer_rect.size() + egui::vec2(17.0, 29.0);
                assert!(
                    (outer_size_with_chrome.x - geometry.0[0]).abs() > tolerance
                        || (outer_size_with_chrome.y - geometry.0[1]).abs() > tolerance,
                    "outer frame/title-bar dimensions must not become the persisted body viewport"
                );
                persistence.window_size = Some(geometry.0);
                persistence.window_position = Some(geometry.1);
            }
        }
    }

    #[test]
    fn direct_render_mutation_is_retained_and_id_is_stable() {
        let mut dialog = folder_dialog(41);
        let id_before = dialog.workspace.current_view.id;
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        render_retained_folder(state, |state| {
            state.path_filter = "mutated while rendering".into();
            folder_view::FolderViewAction::Noop
        });
        assert_eq!(dialog.workspace.current_view.id, id_before);
        assert!(matches!(
            &dialog.workspace.current_view.view,
            DiffView::FolderCompare(state) if state.path_filter == "mutated while rendering"
        ));
    }

    #[test]
    fn replacing_comparison_removes_obsolete_runtime() {
        let mut dialog = folder_dialog(51);
        dialog.folder_runtimes.insert(51, Default::default());
        dialog.watch_views.insert(
            51,
            crate::diff::watch::ViewWatchRuntime::folder(
                crate::diff::watch::WatchTag {
                    workspace: dialog.workspace.workspace_id,
                    view: 51,
                    generation: 1,
                },
                tempfile::tempdir().unwrap().path().into(),
                tempfile::tempdir().unwrap().path().into(),
            ),
        );
        let dirs = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        dialog
            .workspace
            .open_paths(
                dirs.0.path().display().to_string(),
                dirs.1.path().display().to_string(),
            )
            .unwrap();
        dialog.reconcile_runtime_resources();
        assert!(!dialog.folder_runtimes.contains_key(&51));
        assert!(!dialog.watch_views.contains_key(&51));
    }

    #[test]
    fn closing_dialog_drops_all_watchers() {
        let mut dialog = folder_dialog(52);
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        dialog.watch_views.insert(
            52,
            crate::diff::watch::ViewWatchRuntime::folder(
                crate::diff::watch::WatchTag {
                    workspace: dialog.workspace.workspace_id,
                    view: 52,
                    generation: 1,
                },
                left.path().into(),
                right.path().into(),
            ),
        );
        dialog.clear_runtime_resources();
        assert!(dialog.watch_views.is_empty());
    }

    #[test]
    fn back_restores_retained_state_and_its_independent_runtime() {
        let mut dialog = folder_dialog(61);
        if let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view {
            state.path_filter = "kept".into();
        }
        dialog.folder_runtimes.insert(61, Default::default());
        dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
            relative_path: "child".into(),
            left: Some("child".into()),
            right: None,
        });
        assert!(dialog.folder_runtimes.contains_key(&61));
        dialog.apply_folder_action(folder_view::FolderViewAction::NavigateBack);
        assert_eq!(dialog.workspace.current_view.id, 61);
        assert!(dialog.folder_runtimes.contains_key(&61));
        assert!(matches!(
            &dialog.workspace.current_view.view,
            DiffView::FolderCompare(state) if state.path_filter == "kept"
        ));
    }

    #[test]
    fn alt_left_and_button_back_share_the_same_transition() {
        fn opened_dialog() -> DiffDialogState {
            let mut dialog = folder_dialog(71);
            dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
                relative_path: "child.txt".into(),
                left: Some("child.txt".into()),
                right: None,
            });
            dialog
        }

        // The Alt+Left handler calls navigate_back directly; the button emits
        // NavigateBack through apply_folder_action. Both must produce the same
        // retained workspace transition.
        let mut alt = opened_dialog();
        let mut button = opened_dialog();
        assert!(alt.navigate_back());
        button.apply_folder_action(folder_view::FolderViewAction::NavigateBack);
        assert_eq!(
            alt.workspace.current_view.view,
            button.workspace.current_view.view
        );
        assert_eq!(
            alt.workspace.navigation_stack,
            button.workspace.navigation_stack
        );
    }

    #[test]
    fn valid_rescan_applies_draft_clears_scan_data_and_retains_view_controls() {
        let mut dialog = folder_dialog(81);
        let runtime = dialog.folder_runtimes.entry(81).or_default();
        runtime.generation = 3;
        runtime.left_visited = 9;
        if let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view {
            state.draft_rules.include_rules = "*.tmp".into();
            state.draft_rules.exclude_rules = ".git/\ntarget/".into();
            state.path_filter = "needle".into();
            state.display_filter = crate::diff::model::FolderDisplayFilter::Differences;
            state.expanded_nodes.insert("expanded".into());
            state.selected_paths.insert("survivor.tmp".into());
        }
        dialog.apply_folder_action(folder_view::FolderViewAction::RequestRescan);
        let DiffView::FolderCompare(state) = &dialog.workspace.current_view.view else {
            unreachable!()
        };
        assert_eq!(state.applied_scan_rules.includes, ["*.tmp"]);
        assert_eq!(state.applied_scan_rules.excludes, [".git/", "target/"]);
        assert!(state.model.entries.is_empty());
        assert_eq!(state.path_filter, "needle");
        assert!(
            state
                .expanded_nodes
                .contains(std::path::Path::new("expanded"))
        );
        assert!(
            state
                .selected_paths
                .contains(std::path::Path::new("survivor.tmp"))
        );
        let runtime = &dialog.folder_runtimes[&81];
        assert!(runtime.generation > 3 && runtime.restart_prepared);
        assert_eq!(runtime.left_visited, 0);
    }

    #[test]
    fn principal_folder_workflow_uses_injected_controller_boundaries() {
        use crate::diff::file_ops::{ItemOutcome, execute_copy};
        use crate::diff::folder_compare::{EntryKind, EntryMetadata, EntrySide};
        use crate::diff::folder_scan::{DiscoveredEntry, RootIdentity, ScanEvent};
        use crate::diff::model::{DiffSide, FolderDisplayFilter};
        use crate::diff::text_compare::NavigationDirection;
        use std::collections::HashSet;
        use std::sync::atomic::AtomicBool;

        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        std::fs::write(left.path().join("changed.bin"), [0, 1, 2, 3]).unwrap();
        std::fs::write(right.path().join("changed.bin"), [0, 9, 2, 8]).unwrap();
        std::fs::write(left.path().join("copy.txt"), "left version").unwrap();
        // An archive is deliberately one ordinary file event. There is no
        // scanner/archive mount callback capable of yielding member paths.
        std::fs::write(left.path().join("bundle.zip"), b"PK\0opaque").unwrap();

        let mut dialog = DiffDialogState::default();
        let assign_picker = |workspace: &mut DiffWorkspace, side, path: &std::path::Path| {
            workspace.assign_selected_path(side, Some(path.to_path_buf()));
        };
        assign_picker(&mut dialog.workspace, DiffSide::Left, left.path());
        assign_picker(&mut dialog.workspace, DiffSide::Right, right.path());
        dialog
            .open_and_record(
                dialog.workspace.left_visible.clone(),
                dialog.workspace.right_visible.clone(),
            )
            .unwrap();
        let folder_id = dialog.workspace.current_view.id;
        let runtime = dialog.folder_runtimes.entry(folder_id).or_default();
        runtime.generation = 77;
        let left_root = RootIdentity::new(left.path().to_path_buf());
        let right_root = RootIdentity::new(right.path().to_path_buf());
        runtime.left_root = Some(left_root.clone());
        runtime.right_root = Some(right_root.clone());

        let discovered = |root: &std::path::Path, name: &str| DiscoveredEntry {
            relative_path: name.into(),
            side: EntrySide {
                path: root.join(name),
                metadata: Some(EntryMetadata::from_fs(
                    &std::fs::metadata(root.join(name)).unwrap(),
                    EntryKind::File,
                )),
                error: None,
            },
        };
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        // Inject progressive scanner batches instead of starting worker threads.
        process_scan_events(
            state,
            runtime,
            true,
            &left_root,
            [
                ScanEvent::EntriesDiscovered {
                    generation: 77,
                    root: left_root.clone(),
                    entries: vec![discovered(left.path(), "changed.bin")],
                },
                ScanEvent::Progress {
                    generation: 77,
                    root: left_root.clone(),
                    visited: 1,
                },
                ScanEvent::EntriesDiscovered {
                    generation: 77,
                    root: left_root.clone(),
                    entries: vec![
                        discovered(left.path(), "copy.txt"),
                        discovered(left.path(), "bundle.zip"),
                    ],
                },
                ScanEvent::Completed {
                    generation: 77,
                    root: left_root.clone(),
                    visited: 3,
                },
            ],
        );
        process_scan_events(
            state,
            runtime,
            false,
            &right_root,
            [
                ScanEvent::EntriesDiscovered {
                    generation: 77,
                    root: right_root.clone(),
                    entries: vec![discovered(right.path(), "changed.bin")],
                },
                ScanEvent::Completed {
                    generation: 77,
                    root: right_root.clone(),
                    visited: 1,
                },
            ],
        );
        assert_eq!(runtime.left_visited, 3);
        assert!(state.left_scan_complete && state.right_scan_complete);
        assert!(state.model.entries.contains_key("bundle.zip"));
        assert!(
            !state
                .model
                .entries
                .keys()
                .any(|key| key.starts_with("bundle.zip/"))
        );

        state.display_filter = FolderDisplayFilter::Differences;
        state.path_filter = "changed".into();
        assert_eq!(
            folder_view::ordered_visible(state),
            [std::path::PathBuf::from("changed.bin")]
        );
        state.primary_selection = Some("changed.bin".into());
        state.selected_paths.insert("changed.bin".into());
        state.scroll_anchor = state.primary_selection.clone();
        let retained = state.clone();

        dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
            relative_path: "changed.bin".into(),
            left: Some(left.path().join("changed.bin")),
            right: Some(right.path().join("changed.bin")),
        });
        let child_id = dialog.workspace.current_view.id;
        let DiffView::BinaryCompare(child) = &dialog.workspace.current_view.view else {
            panic!("binary child must route to the read-only hex comparator")
        };
        let mut binary = crate::diff::binary_compare::BinaryViewModel::load(child, 0.5).unwrap();
        binary.navigate(NavigationDirection::Next);
        binary.navigate(NavigationDirection::Last);
        assert!(binary.current_difference.is_some());
        dialog.binary_views.insert(child_id, binary);
        assert!(dialog.navigate_back());
        assert!(
            matches!(&dialog.workspace.current_view.view, DiffView::FolderCompare(s) if s == &retained)
        );

        // Capture a new selection, preview it, and pass the immutable plan to
        // an injected synchronous executor (no GUI click or worker timing).
        {
            let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
                unreachable!()
            };
            state.path_filter.clear();
            state.selected_paths.clear();
            state.selected_paths.insert("copy.txt".into());
            state.primary_selection = Some("copy.txt".into());
        }
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        let preview = dialog.operation_preview.take().expect("copy preview");
        let operation_context = preview.context;
        let operation_preview::PreviewPlan::Copy(plan) = preview.plan else {
            unreachable!()
        };
        let _pending = pending_operation(
            dialog.folder_runtimes.get_mut(&folder_id).unwrap(),
            &operation_context,
        );
        let execute = |plan| execute_copy(plan, &HashSet::new(), &AtomicBool::new(false));
        let report = execute(&plan);
        assert!(matches!(report.items[0].outcome, ItemOutcome::Copied));
        assert_eq!(
            std::fs::read_to_string(right.path().join("copy.txt")).unwrap(),
            "left version"
        );

        let DiffDialogState {
            workspace,
            folder_runtimes,
            ..
        } = &mut dialog;
        let DiffView::FolderCompare(state) = &mut workspace.current_view.view else {
            unreachable!()
        };
        let runtime = folder_runtimes.get_mut(&folder_id).unwrap();
        process_operation_events(
            folder_id,
            state,
            runtime,
            &operation_context,
            [crate::diff::file_ops::OperationEvent::Completed(report)],
        );
        assert_eq!(runtime.operation_feedback.as_ref().unwrap().completed, 1);
        assert!(state.model.entries.is_empty());
        assert!(
            state
                .selected_paths
                .contains(std::path::Path::new("copy.txt"))
        );
        // Inject the refreshed surviving row, then apply the same reconciliation
        // performed after completed scans.
        process_scan_events(
            state,
            runtime,
            false,
            &RootIdentity::new(right.path().to_path_buf()),
            std::iter::empty(),
        );
        assert!(
            state
                .selected_paths
                .contains(std::path::Path::new("copy.txt"))
        );
    }

    #[test]
    fn gui_delete_planning_is_recycle_only() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("remove.txt"), "x").unwrap();
        let mut dialog = folder_dialog(901);
        dialog.folder_runtimes.entry(901).or_default().generation = 4;
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        state.left_root = root.path().into();
        state.right_root = other.path().into();
        state.selected_paths.insert("remove.txt".into());
        state
            .model
            .upsert(
                std::path::Path::new("remove.txt"),
                crate::diff::folder_compare::EntrySide {
                    path: root.path().join("remove.txt"),
                    metadata: None,
                    error: None,
                },
                true,
                crate::diff::folder_compare::PathKeyPolicy::Platform,
                state.timestamp_tolerance,
            )
            .unwrap();
        dialog.plan_mutation(folder_view::MutationKind::DeleteLeft);
        let preview = dialog.operation_preview.as_ref().unwrap();
        let operation_preview::PreviewPlan::Delete(plan) = &preview.plan else {
            unreachable!()
        };
        assert_eq!(plan.mode, crate::diff::file_ops::DeleteMode::Recycle);
    }

    fn operation_fixture() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        DiffDialogState,
        FolderOperationContext,
    ) {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        std::fs::write(left.path().join("item.txt"), b"copyable source").unwrap();
        let mut dialog = folder_dialog(950);
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        state.left_root = left.path().into();
        state.right_root = right.path().into();
        let context = FolderOperationContext {
            comparison: folder_comparison_identity(950, state),
            generation: 42,
        };
        let runtime = dialog.folder_runtimes.entry(950).or_default();
        runtime.generation = context.generation;
        runtime.reconcile_comparison(context.comparison.clone());
        select_operation_item(&mut dialog);
        (left, right, dialog, context)
    }

    fn pending_operation(
        runtime: &mut crate::diff::folder_runtime::FolderRuntime,
        context: &FolderOperationContext,
    ) -> (
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        std::sync::mpsc::Sender<OperationEvent>,
    ) {
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (sender, receiver) = std::sync::mpsc::channel();
        runtime.start_operation(
            context.clone(),
            crate::diff::file_ops::OperationHandle::with_test_channel(receiver, cancel.clone()),
        );
        (cancel, sender)
    }

    fn pending_operation_fixture() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        DiffDialogState,
        FolderOperationContext,
    ) {
        let (left, right, mut dialog, context) = operation_fixture();
        pending_operation(dialog.folder_runtimes.get_mut(&950).unwrap(), &context);
        (left, right, dialog, context)
    }

    fn consume_operation_events(dialog: &mut DiffDialogState) {
        let view_id = dialog.workspace.current_view.id;
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        let runtime = dialog.folder_runtimes.get_mut(&view_id).unwrap();
        let (context, events) = runtime.operation_events().expect("pending operation");
        process_operation_events(view_id, state, runtime, &context, events);
    }

    fn select_operation_item(dialog: &mut DiffDialogState) {
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        state
            .model
            .upsert(
                std::path::Path::new("item.txt"),
                crate::diff::folder_compare::EntrySide {
                    path: state.left_root.join("item.txt"),
                    metadata: None,
                    error: None,
                },
                true,
                crate::diff::folder_compare::PathKeyPolicy::Platform,
                state.timestamp_tolerance,
            )
            .unwrap();
        state.selected_paths.insert("item.txt".into());
        state.primary_selection = Some("item.txt".into());
    }

    fn operation_report(
        context: &FolderOperationContext,
        outcomes: Vec<ItemOutcome>,
        cancelled: bool,
    ) -> ExecutionReport {
        ExecutionReport {
            generation: context.generation,
            items: outcomes
                .into_iter()
                .enumerate()
                .map(|(index, outcome)| ItemResult {
                    relative: format!("entry-{index}.txt").into(),
                    outcome,
                })
                .collect(),
            affected_subtree: None,
            cancelled,
        }
    }

    fn deliver_operation_report(
        dialog: &mut DiffDialogState,
        context: &FolderOperationContext,
        report: ExecutionReport,
    ) {
        let view_id = dialog.workspace.current_view.id;
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        let runtime = dialog.folder_runtimes.entry(view_id).or_default();
        process_operation_events(
            view_id,
            state,
            runtime,
            context,
            [OperationEvent::Completed(report)],
        );
    }

    #[test]
    fn folder_operation_mixed_results_keep_exact_counts_and_bounded_failures() {
        let (_left, _right, mut dialog, context) = pending_operation_fixture();
        dialog.workspace.error = Some("unrelated existing error".into());
        let mut outcomes = vec![
            ItemOutcome::CreatedDirectory,
            ItemOutcome::Copied,
            ItemOutcome::Overwritten,
        ];
        let limit = crate::diff::folder_runtime::MAX_OPERATION_FAILURE_DETAILS;
        outcomes.extend(
            (0..limit + 3).map(|index| ItemOutcome::Failed(format!("read failure {index}"))),
        );
        let report = operation_report(&context, outcomes, false);
        deliver_operation_report(&mut dialog, &context, report.clone());
        let runtime = &dialog.folder_runtimes[&950];
        let feedback = runtime.operation_feedback.as_ref().unwrap().clone();
        assert_eq!((feedback.completed, feedback.failed), (3, limit + 3));
        assert_eq!(feedback.failures.len(), limit);
        assert_eq!(feedback.omitted_failures, 3);
        assert_eq!(
            feedback.failures[0].relative,
            std::path::PathBuf::from("entry-3.txt")
        );
        assert_eq!(feedback.failures[0].message, "read failure 0");
        assert!(!feedback.cancelled);
        assert_eq!(
            dialog.workspace.error.as_deref(),
            Some("unrelated existing error")
        );
        assert!(
            matches!(&dialog.workspace.current_view.view, DiffView::FolderCompare(state) if state.model.entries.is_empty())
        );
        let generation = runtime.generation;
        deliver_operation_report(&mut dialog, &context, report);
        assert_eq!(dialog.folder_runtimes[&950].generation, generation);
        assert_eq!(
            dialog.folder_runtimes[&950].operation_feedback.as_ref(),
            Some(&feedback)
        );
    }

    #[test]
    fn folder_operation_cancellation_counts_only_delivered_successes() {
        for (outcomes, cancelled, completed) in [
            (vec![], true, 0),
            (vec![ItemOutcome::Copied], true, 1),
            (vec![ItemOutcome::Copied, ItemOutcome::Cancelled], true, 1),
            (vec![ItemOutcome::Cancelled], false, 0),
        ] {
            let (_left, _right, mut dialog, context) = pending_operation_fixture();
            deliver_operation_report(
                &mut dialog,
                &context,
                operation_report(&context, outcomes, cancelled),
            );
            let feedback = dialog.folder_runtimes[&950]
                .operation_feedback
                .as_ref()
                .unwrap();
            assert!(feedback.cancelled);
            assert_eq!((feedback.completed, feedback.failed), (completed, 0));
            assert_eq!(
                folder_operation_summary(feedback),
                format!("Folder operation cancelled: {completed} completed, 0 failed.")
            );
        }
    }

    #[test]
    fn folder_operation_full_success_uses_neutral_completed_summary() {
        let (_left, _right, mut dialog, context) = pending_operation_fixture();
        deliver_operation_report(
            &mut dialog,
            &context,
            operation_report(
                &context,
                vec![ItemOutcome::Copied, ItemOutcome::Overwritten],
                false,
            ),
        );
        let feedback = dialog.folder_runtimes[&950]
            .operation_feedback
            .as_ref()
            .unwrap();
        assert!(!feedback.cancelled);
        assert!(feedback.failures.is_empty());
        assert_eq!(
            folder_operation_summary(feedback),
            "Folder operation: 2 completed, 0 failed."
        );
    }

    #[test]
    fn folder_operation_rejects_wrong_generation_view_and_ordered_roots() {
        for mismatch in 0..5 {
            let (_left, _right, mut dialog, context) = pending_operation_fixture();
            let mut received_context = context.clone();
            let mut report =
                operation_report(&context, vec![ItemOutcome::Failed("stale".into())], false);
            match mismatch {
                0 => report.generation += 1,
                1 => received_context.generation += 1,
                2 => received_context.comparison.view_id += 1,
                3 => std::mem::swap(
                    &mut received_context.comparison.left_root,
                    &mut received_context.comparison.right_root,
                ),
                _ => received_context.comparison.right_root.token = Some((u64::MAX, u64::MAX)),
            }
            deliver_operation_report(&mut dialog, &received_context, report);
            let runtime = &dialog.folder_runtimes[&950];
            assert!(runtime.operation_feedback.is_none(), "mismatch {mismatch}");
            assert!(
                runtime.mutation_active(),
                "mismatch must not release the gate"
            );
            assert_eq!(
                runtime.generation, context.generation,
                "must not trigger completion rescan"
            );
            assert!(
                matches!(&dialog.workspace.current_view.view, DiffView::FolderCompare(state) if state.model.entries.contains_key("item.txt"))
            );
        }
    }

    #[test]
    fn folder_operation_feedback_survives_rescan_preview_cancel_and_unchanged_parent() {
        let (left, _right, mut dialog, context) = pending_operation_fixture();
        deliver_operation_report(
            &mut dialog,
            &context,
            operation_report(
                &context,
                vec![ItemOutcome::Failed("preserved".into())],
                false,
            ),
        );
        let feedback = dialog.folder_runtimes[&950].operation_feedback.clone();
        dialog.apply_folder_action(folder_view::FolderViewAction::RequestRescan);
        assert!(dialog.folder_runtimes[&950].left_root.is_none());
        assert_eq!(dialog.folder_runtimes[&950].operation_feedback, feedback);
        select_operation_item(&mut dialog);
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        assert!(dialog.operation_preview.is_some());
        assert_eq!(dialog.folder_runtimes[&950].operation_feedback, feedback);
        dialog.apply_operation_preview_response(operation_preview::PreviewResponse::Cancel);
        assert!(dialog.operation_preview.is_none());
        assert_eq!(dialog.folder_runtimes[&950].operation_feedback, feedback);
        dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
            relative_path: "item.txt".into(),
            left: Some(left.path().join("item.txt")),
            right: None,
        });
        assert_eq!(dialog.folder_runtimes[&950].operation_feedback, feedback);
        assert!(dialog.navigate_back());
        assert_eq!(dialog.folder_runtimes[&950].operation_feedback, feedback);
    }

    #[test]
    fn folder_operation_new_dispatch_clears_previous_feedback_and_replaces_it_on_completion() {
        let (_left, right, mut dialog, context) = pending_operation_fixture();
        deliver_operation_report(
            &mut dialog,
            &context,
            operation_report(
                &context,
                vec![ItemOutcome::Failed("old failure".into())],
                false,
            ),
        );
        select_operation_item(&mut dialog);
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        let dispatched = dialog.operation_preview.as_ref().unwrap().context.clone();
        dialog.apply_operation_preview_response(operation_preview::PreviewResponse::Execute);
        let runtime = &dialog.folder_runtimes[&950];
        assert!(runtime.mutation_active());
        assert!(runtime.operation_feedback.is_none());
        // Observe the small real copy before temp-directory cleanup; other report
        // cases inject events directly through the production adoption seam.
        let event = runtime.wait_for_operation_event();
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        let runtime = dialog.folder_runtimes.get_mut(&950).unwrap();
        process_operation_events(950, state, runtime, &dispatched, [event]);
        assert!(!runtime.mutation_active());
        let feedback = runtime.operation_feedback.as_ref().unwrap();
        assert_eq!((feedback.completed, feedback.failed), (1, 0));
        assert_eq!(
            std::fs::read(right.path().join("item.txt")).unwrap(),
            b"copyable source"
        );
    }

    #[test]
    fn folder_operation_rescan_holds_busy_gate_until_matching_terminal_acknowledgement() {
        let (_left, right, mut dialog, context) = operation_fixture();
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        let mut preview = dialog.operation_preview.take().unwrap();
        let (cancel, sender) =
            pending_operation(dialog.folder_runtimes.get_mut(&950).unwrap(), &context);

        dialog.refresh_folder();
        let generation = dialog.folder_runtimes[&950].generation;
        assert!(generation > context.generation);
        assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
        assert!(dialog.folder_runtimes[&950].mutation_active());
        assert!(dialog.folder_runtimes[&950].is_active());
        select_operation_item(&mut dialog);
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        assert!(
            dialog.operation_preview.is_none(),
            "planning remains blocked"
        );

        // A current preview at the dispatch boundary must also respect the gate,
        // independently of the stale-preview generation check.
        preview.context.generation = generation;
        let operation_preview::PreviewPlan::Copy(plan) = &mut preview.plan else {
            unreachable!()
        };
        plan.generation = generation;
        assert!(dialog.operation_preview_is_current(&preview));
        dialog.operation_preview = Some(preview);
        dialog.apply_operation_preview_response(operation_preview::PreviewResponse::Execute);
        assert!(dialog.folder_runtimes[&950].mutation_active());
        assert_eq!(
            dialog.folder_runtimes[&950].operation_events().unwrap().0,
            context,
            "dispatch must not replace the pending operation"
        );
        assert!(!right.path().join("item.txt").exists());

        sender
            .send(OperationEvent::Progress {
                generation: context.generation,
                completed: 1,
                total: 2,
            })
            .unwrap();
        consume_operation_events(&mut dialog);
        assert!(dialog.folder_runtimes[&950].mutation_active());
        sender
            .send(OperationEvent::Completed(operation_report(
                &context,
                vec![ItemOutcome::Copied],
                true,
            )))
            .unwrap();
        consume_operation_events(&mut dialog);
        let runtime = &dialog.folder_runtimes[&950];
        assert!(!runtime.mutation_active());
        assert!(
            runtime.operation_feedback.is_none(),
            "stale result not adopted"
        );
        assert_eq!(runtime.generation, generation, "no completion rescan");
        assert!(
            matches!(&dialog.workspace.current_view.view, DiffView::FolderCompare(state) if state.model.entries.contains_key("item.txt"))
        );
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        assert!(
            dialog.operation_preview.is_some(),
            "acknowledgement releases gate"
        );
        dialog.apply_operation_preview_response(operation_preview::PreviewResponse::Cancel);
    }

    #[test]
    fn folder_operation_wrong_terminal_cannot_release_a_newer_active_dispatch() {
        let (_left, _right, mut dialog, old_context) = operation_fixture();
        let (_old_cancel, old_sender) =
            pending_operation(dialog.folder_runtimes.get_mut(&950).unwrap(), &old_context);
        dialog.refresh_folder();
        old_sender
            .send(OperationEvent::Completed(operation_report(
                &old_context,
                vec![],
                true,
            )))
            .unwrap();
        consume_operation_events(&mut dialog);
        assert!(!dialog.folder_runtimes[&950].mutation_active());

        let mut context = old_context.clone();
        context.generation = dialog.folder_runtimes[&950].generation;
        let (_cancel, sender) =
            pending_operation(dialog.folder_runtimes.get_mut(&950).unwrap(), &context);
        deliver_operation_report(
            &mut dialog,
            &old_context,
            operation_report(&old_context, vec![ItemOutcome::Copied], true),
        );
        assert!(dialog.folder_runtimes[&950].mutation_active());
        let mut wrong_report = operation_report(&context, vec![ItemOutcome::Copied], false);
        wrong_report.generation = old_context.generation;
        sender
            .send(OperationEvent::Completed(wrong_report))
            .unwrap();
        consume_operation_events(&mut dialog);
        let runtime = &dialog.folder_runtimes[&950];
        assert!(runtime.mutation_active());
        assert_eq!(runtime.generation, context.generation);
        assert!(runtime.operation_feedback.is_none());
        sender
            .send(OperationEvent::Completed(operation_report(
                &context,
                vec![ItemOutcome::Copied],
                false,
            )))
            .unwrap();
        consume_operation_events(&mut dialog);
        let runtime = &dialog.folder_runtimes[&950];
        assert!(!runtime.mutation_active());
        assert_eq!(runtime.operation_feedback.as_ref().unwrap().completed, 1);
        assert!(runtime.generation > context.generation);
    }

    #[test]
    fn folder_operation_stale_preview_cannot_dispatch_after_root_change() {
        let (_left, right, mut dialog, _context) = operation_fixture();
        dialog.plan_mutation(folder_view::MutationKind::CopyRight);
        let replacement = tempfile::tempdir().unwrap();
        let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view else {
            unreachable!()
        };
        state.right_root = replacement.path().into();
        dialog.apply_operation_preview_response(operation_preview::PreviewResponse::Execute);
        assert!(!dialog.folder_runtimes[&950].mutation_active());
        assert!(!right.path().join("item.txt").exists());
        assert!(!replacement.path().join("item.txt").exists());
    }

    #[test]
    fn folder_operation_discard_and_same_id_side_swap_clear_feedback() {
        for discard in [false, true] {
            let (_left, _right, mut dialog, context) = pending_operation_fixture();
            deliver_operation_report(
                &mut dialog,
                &context,
                operation_report(&context, vec![ItemOutcome::Copied], false),
            );
            if discard {
                dialog.workspace.current_view.view = DiffView::Start;
                dialog.reconcile_runtime_resources();
            } else {
                dialog.swap_sides();
                assert_eq!(dialog.workspace.current_view.id, 950);
            }
            assert!(!dialog.folder_runtimes.contains_key(&950));
        }
    }

    #[test]
    fn folder_operation_retained_parent_swap_clears_feedback_while_child_is_open() {
        let (left, _right, mut dialog, context) = pending_operation_fixture();
        deliver_operation_report(
            &mut dialog,
            &context,
            operation_report(&context, vec![ItemOutcome::Copied], false),
        );
        dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
            relative_path: "item.txt".into(),
            left: Some(left.path().join("item.txt")),
            right: None,
        });
        assert!(dialog.folder_runtimes.contains_key(&950));
        dialog.swap_sides();
        assert!(!dialog.folder_runtimes.contains_key(&950));
    }

    #[test]
    fn folder_operation_changed_retained_parent_rejects_queued_completion_before_reconciliation() {
        let (left, _right, mut dialog, context) = pending_operation_fixture();
        let queued = operation_report(
            &context,
            vec![ItemOutcome::Failed("old roots".into())],
            false,
        );
        dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
            relative_path: "item.txt".into(),
            left: Some(left.path().join("item.txt")),
            right: None,
        });
        let replacement = tempfile::tempdir().unwrap();
        let DiffView::FolderCompare(parent) =
            &mut dialog.workspace.navigation_stack.last_mut().unwrap().view
        else {
            unreachable!()
        };
        parent.right_root = replacement.path().into();
        assert!(dialog.workspace.back()); // deliberately before dialog reconciliation
        deliver_operation_report(&mut dialog, &context, queued);
        assert!(dialog.folder_runtimes[&950].operation_feedback.is_none());
        assert!(
            matches!(&dialog.workspace.current_view.view, DiffView::FolderCompare(state) if state.model.entries.contains_key("item.txt"))
        );
    }

    #[test]
    fn folder_operation_reconciliation_drops_changed_retained_comparison() {
        let (left, _right, mut dialog, context) = pending_operation_fixture();
        deliver_operation_report(
            &mut dialog,
            &context,
            operation_report(&context, vec![ItemOutcome::Copied], false),
        );
        dialog.apply_folder_action(folder_view::FolderViewAction::OpenChild {
            relative_path: "item.txt".into(),
            left: Some(left.path().join("item.txt")),
            right: None,
        });
        let replacement = tempfile::tempdir().unwrap();
        let DiffView::FolderCompare(parent) =
            &mut dialog.workspace.navigation_stack.last_mut().unwrap().view
        else {
            unreachable!()
        };
        parent.left_root = replacement.path().into();
        dialog.reconcile_runtime_resources();
        assert!(!dialog.folder_runtimes.contains_key(&950));
    }

    enum ProductionFixture {
        Text(Box<crate::diff::model::TextViewModel>),
        Folder(
            Box<FolderCompareState>,
            Vec<crate::diff::folder_compare::FolderProjectionRow>,
        ),
        Binary(Box<crate::diff::binary_compare::BinaryViewModel>),
    }

    impl ProductionFixture {
        fn pathological(kind: usize) -> Self {
            let token = "very-long-component-".repeat(300);
            match kind {
                0 => Self::Text(Box::new(crate::diff::model::TextViewModel::from_test_text(
                    format!("left-{token}"),
                    format!("right-{token}"),
                ))),
                1 => {
                    let mut state = FolderCompareState::default();
                    state.left_root = token.clone().into();
                    state.right_root = format!("right-{token}").into();
                    // Deliberately wide production columns remain owned by the table scroller.
                    state.column_widths = crate::diff::settings::FolderColumnWidthsV1 {
                        left_path: 900.0,
                        right_path: 900.0,
                        ..Default::default()
                    };
                    let rows = (0..40)
                        .map(|index| {
                            let path: std::path::PathBuf = format!("{token}/{index}").into();
                            state.model.entries.insert(
                                path.to_string_lossy().into_owned(),
                                crate::diff::folder_compare::FolderEntry {
                                    relative_path: path.clone(),
                                    left: None,
                                    right: None,
                                    metadata_status:
                                        crate::diff::folder_compare::FolderStatus::Different,
                                    effective_status:
                                        crate::diff::folder_compare::FolderStatus::Different,
                                    content_checked: true,
                                },
                            );
                            crate::diff::folder_compare::FolderProjectionRow {
                                path,
                                depth: 0,
                                has_children: false,
                            }
                        })
                        .collect();
                    Self::Folder(Box::new(state), rows)
                }
                2 => Self::Binary(Box::new(
                    crate::diff::binary_compare::BinaryViewModel::from_test_bytes(
                        (0..4096).map(|n| n as u8).collect(),
                        (0..4096).map(|n| (n as u8).wrapping_add(1)).collect(),
                    ),
                )),
                _ => unreachable!(),
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct ProductionGeometry {
        outer: egui::Rect,
        body: egui::Rect,
        clip: egui::Rect,
        viewport_width: f32,
        content_width: f32,
    }

    fn render_production_frame(
        ctx: &egui::Context,
        fixture: &mut ProductionFixture,
        requested_size: egui::Vec2,
        id: u64,
    ) -> ProductionGeometry {
        let mut measured = ProductionGeometry {
            outer: egui::Rect::NOTHING,
            body: egui::Rect::NOTHING,
            clip: egui::Rect::NOTHING,
            viewport_width: 0.0,
            content_width: 0.0,
        };
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            requested_size + egui::vec2(200.0, 200.0),
        ));
        for input in [input.clone(), input] {
            ctx.run_ui(input, |ctx| {
                let response = egui::Window::new("Diff production layout regression")
                    .id(egui::Id::new(("diff-production-layout", id)))
                    .collapsible(false)
                    .resizable(false)
                    .default_pos(egui::pos2(40.0, 30.0))
                    // Each test frame represents a settled user resize. A fixed
                    // size avoids `default_size` being treated as a first-frame
                    // hint and then recomputed from the production content.
                    .fixed_size(requested_size + window_chrome(ctx.ctx()))
                    .show(ctx, |ui| {
                        // Match `DiffDialogState::ui`: reserve the complete body before
                        // laying out any production child. Otherwise a long child can
                        // negotiate the Window size before the test observes the body.
                        let body_viewport = DiffViewport::remaining(ui);
                        let body = allocate_viewport(ui, body_viewport, |ui| {
                            measured.clip = ui.clip_rect().intersect(body_viewport.rect);
                            // Exercise the bounded path/error behavior with pathological labels.
                            let row_height = ui.spacing().interact_size.y;
                            egui::ScrollArea::horizontal()
                                .max_height(row_height)
                                .show(ui, |ui| {
                                    ui.label("left-path".repeat(500));
                                    ui.label("↔");
                                    ui.label("right-path".repeat(500));
                                    let _ = ui.button("Compare");
                                    let _ = ui.menu_button("More", |_| {});
                                });
                            bounded_message(ui, &"workspace-error".repeat(500), 48.0);
                            ui.separator();
                            allocate_remaining_workspace(ui, |ui| {
                                let workspace_width = ui.max_rect().width();
                                match fixture {
                                    ProductionFixture::Text(model) => {
                                        text_view::show(ui, 1, id, model);
                                        let pane = ((workspace_width - 8.0) * 0.5).max(0.0);
                                        measured.viewport_width = pane;
                                        measured.content_width = text_view::unwrapped_content_width(
                                            model.left.source().lines().next().unwrap_or(""),
                                            8.5,
                                            72.0,
                                        );
                                    }
                                    ProductionFixture::Folder(state, rows) => {
                                        let paths = rows
                                            .iter()
                                            .map(|row| row.path.clone())
                                            .collect::<Vec<_>>();
                                        let mut action = folder_view::FolderViewAction::Noop;
                                        let geometry = folder_table::show(
                                            ui,
                                            state,
                                            &paths,
                                            rows,
                                            false,
                                            &mut action,
                                        );
                                        measured.viewport_width = geometry.viewport.width();
                                        measured.content_width = geometry.content_width;
                                    }
                                    ProductionFixture::Binary(model) => {
                                        let geometry = binary_view::show(ui, 1, id, model);
                                        measured.viewport_width = geometry.viewport.width();
                                        measured.content_width = geometry.content_size.x;
                                    }
                                }
                            });
                        });
                        measured.body = body.response.rect;
                    })
                    .unwrap();
                measured.outer = response.response.rect;
            })
            .drop_without_applying_deltas();
        }
        measured
    }

    fn assert_rect_close(actual: egui::Rect, expected: egui::Rect) {
        let tolerance = 1.0;
        assert!(
            (actual.min.x - expected.min.x).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.min.y - expected.min.y).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.max.x - expected.max.x).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.max.y - expected.max.y).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
    }

    fn rect_contains_with_tolerance(outer: egui::Rect, inner: egui::Rect) -> bool {
        let tolerance = 1.0;
        inner.min.x >= outer.min.x - tolerance
            && inner.min.y >= outer.min.y - tolerance
            && inner.max.x <= outer.max.x + tolerance
            && inner.max.y <= outer.max.y + tolerance
    }

    #[test]
    fn pathological_production_views_keep_window_and_body_bounded() {
        for requested in [egui::vec2(900.0, 650.0), egui::vec2(400.0, 250.0)] {
            let mut baseline = None;
            for kind in 0..3 {
                let ctx = egui::Context::default();
                let mut fixture = ProductionFixture::pathological(kind);
                let geometry = render_production_frame(&ctx, &mut fixture, requested, kind as u64);
                assert!(
                    rect_contains_with_tolerance(geometry.outer, geometry.body),
                    "{geometry:?}"
                );
                assert!(
                    rect_contains_with_tolerance(geometry.body, geometry.clip),
                    "{geometry:?}"
                );
                assert!(geometry.clip.is_positive(), "{geometry:?}");
                assert!(
                    (geometry.body.width() - requested.x).abs() <= 1.0,
                    "{geometry:?}"
                );
                assert!(
                    geometry.content_width > geometry.viewport_width,
                    "{geometry:?}"
                );
                if let Some(expected) = baseline {
                    assert_rect_close(geometry.outer, expected);
                } else {
                    baseline = Some(geometry.outer);
                }
            }
        }
    }

    #[test]
    fn manual_resize_owns_workspace_geometry_in_every_production_mode() {
        for kind in 0..3 {
            let mut sizes = Vec::new();
            for (index, requested) in [egui::vec2(600.0, 350.0), egui::vec2(1000.0, 700.0)]
                .into_iter()
                .enumerate()
            {
                let ctx = egui::Context::default();
                let mut fixture = ProductionFixture::pathological(kind);
                let geometry = render_production_frame(
                    &ctx,
                    &mut fixture,
                    requested,
                    (kind * 10 + index) as u64,
                );
                assert!((geometry.body.width() - requested.x).abs() <= 1.0);
                assert!(geometry.content_width > geometry.viewport_width);
                sizes.push(geometry.body.size());
            }
            assert!(sizes[1].x > sizes[0].x);
            assert!(sizes[1].y > sizes[0].y);
        }
    }

    #[test]
    fn header_editors_share_only_flexible_width() {
        assert_eq!(header_path_width(-1.0, 8.0), 24.0);
        for width in [400.0, 900.0, 1600.0] {
            let editor = header_path_width(width, 8.0);
            assert!(editor >= 24.0);
            assert!(editor * 2.0 <= width.max(48.0));
        }
    }

    #[test]
    fn memory_text_explicit_paste_reads_once_and_preserves_input_on_failure_or_oversize() {
        use std::cell::Cell;

        let mut accepted = "accepted".to_string();
        let reads = Cell::new(0);
        paste_memory_input(&mut accepted, || {
            reads.set(reads.get() + 1);
            Ok("pasted α\r\ntext".into())
        })
        .unwrap();
        assert_eq!(reads.get(), 1);
        assert_eq!(accepted, "pasted α\r\ntext");

        let before = accepted.clone();
        let reads = Cell::new(0);
        assert!(
            paste_memory_input(&mut accepted, || {
                reads.set(reads.get() + 1);
                Err("clipboard has no plain text".into())
            })
            .is_err()
        );
        assert_eq!(reads.get(), 1);
        assert_eq!(accepted, before);

        let reads = Cell::new(0);
        let too_large = "λ".repeat(crate::diff::model::MEMORY_TEXT_MAX_BYTES / 2 + 1);
        assert!(
            paste_memory_input(&mut accepted, || {
                reads.set(reads.get() + 1);
                Ok(too_large)
            })
            .is_err()
        );
        assert_eq!(reads.get(), 1);
        assert_eq!(accepted, before);
    }

    #[test]
    fn memory_text_input_renders_two_editors_and_keeps_invalid_draft_open() {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(screen);
        let mut dialog = DiffDialogState::default();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let draft = dialog.text_input.as_mut().unwrap();
        draft.left = "typed left".into();
        draft.right = "typed right".into();
        let frame = context.run_ui(input, |ctx| dialog.show_text_input(ctx));
        assert!(!frame.shapes.is_empty());
        frame.drop_without_applying_deltas();
        assert_eq!(dialog.text_input.as_ref().unwrap().left, "typed left");
        assert_eq!(dialog.text_input.as_ref().unwrap().right, "typed right");

        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "x".repeat(crate::diff::model::MEMORY_TEXT_MAX_BYTES + 1);
        let (mut draft, error) = dialog.apply_text_input(draft).unwrap_err();
        draft.error = Some(error);
        assert!(dialog.workspace.navigation_stack.is_empty());
        assert_eq!(dialog.text_views.len(), 0);
        assert!(draft.error.is_some());
        assert!(draft.left.len() > crate::diff::model::MEMORY_TEXT_MAX_BYTES);
    }

    #[test]
    fn memory_text_comparisons_are_retained_private_and_absent_from_session_history() {
        let mut dialog = DiffDialogState::default();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "first scratch sentinel".into();
        draft.right = "first right".into();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let first_id = dialog.workspace.current_view.id;
        assert_eq!(dialog.persistence.recent_comparisons.len(), 0);
        assert!(!dialog.watch_views.contains_key(&first_id));
        assert!(dialog.snapshot_session("scratch".into()).is_err());

        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "second left".into();
        draft.right = String::new();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let second_id = dialog.workspace.current_view.id;
        assert_ne!(first_id, second_id);
        assert_eq!(
            dialog.workspace.navigation_stack.last().unwrap().id,
            first_id
        );
        assert!(dialog.text_views.contains_key(&first_id));

        assert!(dialog.navigate_back());
        assert_eq!(dialog.workspace.current_view.id, first_id);
        assert_eq!(
            dialog.text_views.get(&first_id).unwrap().left.source(),
            "first scratch sentinel"
        );
        assert!(dialog.snapshot_session("scratch".into()).is_err());
        let debug = format!("{:?}", dialog.workspace);
        assert!(!debug.contains("first scratch sentinel"));
    }

    #[test]
    fn m3c_selected_raw_lines_compare_with_one_clipboard_read_and_back_restores_original() {
        use crate::diff::text_compare::IgnoredLineRange;
        use std::cell::Cell;

        let (mut dialog, origin_id) =
            memory_dialog_with_inputs("α\r\nignored\r\nlast\r\n", "original right");
        let origin = dialog.text_views.get_mut(&origin_id).unwrap();
        origin
            .left
            .replace_source("edited α\r\nignored\r\nlast\r\n".into())
            .unwrap();
        origin
            .left
            .set_ignored_line_ranges(vec![IgnoredLineRange { start: 1, end: 2 }])
            .unwrap();
        assert!(origin.select_source_line(crate::diff::model::DiffSide::Left, 1, false));
        let original_source = origin.left.source().to_owned();
        let original_revision = origin.left.revision;
        let original_dirty = origin.left.is_dirty();
        let original_has_undo = origin.left.can_undo();
        let original_selection = origin.source_line_selection;
        let original_ignored = origin.left.ignored_line_ranges().to_vec();
        let original_recents = dialog.persistence.recent_comparisons.clone();
        let reads = Cell::new(0);

        dialog
            .compare_selection_with_clipboard_using(origin_id, || {
                reads.set(reads.get() + 1);
                Ok("clipboard\r\ntext".into())
            })
            .unwrap();

        assert_eq!(reads.get(), 1);
        let comparison_id = dialog.workspace.current_view.id;
        assert_ne!(comparison_id, origin_id);
        let comparison = dialog.text_views.get(&comparison_id).unwrap();
        assert_eq!(comparison.left.source(), "ignored\r\n");
        assert_eq!(comparison.right.source(), "clipboard\r\ntext");
        assert_eq!(
            dialog.workspace.navigation_stack.last().unwrap().id,
            origin_id
        );
        assert_eq!(dialog.persistence.recent_comparisons, original_recents);

        assert!(dialog.navigate_back());
        assert_eq!(dialog.workspace.current_view.id, origin_id);
        let origin = dialog.text_views.get(&origin_id).unwrap();
        assert_eq!(origin.left.source(), original_source);
        assert_eq!(origin.left.revision, original_revision);
        assert_eq!(origin.left.is_dirty(), original_dirty);
        assert_eq!(origin.left.can_undo(), original_has_undo);
        assert_eq!(origin.source_line_selection, original_selection);
        assert_eq!(origin.left.ignored_line_ranges(), original_ignored);
    }

    #[test]
    fn m3c_clipboard_compare_rejects_missing_selection_and_preserves_on_error_or_oversize() {
        use std::cell::Cell;

        let (mut dialog, origin_id) = memory_dialog_with_inputs("left\nselected", "right");
        let initial_navigation = dialog.workspace.navigation_stack.clone();
        let initial_sources = (
            dialog.text_views[&origin_id].left.source().to_owned(),
            dialog.text_views[&origin_id].right.source().to_owned(),
        );
        let initial_source_ids = (
            dialog.text_views[&origin_id].left.source_id(),
            dialog.text_views[&origin_id].right.source_id(),
        );
        let reads = Cell::new(0);
        let error = dialog
            .compare_selection_with_clipboard_using(origin_id, || {
                reads.set(reads.get() + 1);
                Ok("clipboard".into())
            })
            .unwrap_err();
        assert!(error.contains("Select one or more source lines"));
        assert_eq!(reads.get(), 0, "missing selection must not read clipboard");
        assert_eq!(dialog.workspace.current_view.id, origin_id);
        assert_eq!(dialog.workspace.navigation_stack, initial_navigation);
        assert_eq!(
            (
                dialog.text_views[&origin_id].left.source(),
                dialog.text_views[&origin_id].right.source(),
            ),
            (initial_sources.0.as_str(), initial_sources.1.as_str())
        );
        assert_eq!(
            (
                dialog.text_views[&origin_id].left.source_id(),
                dialog.text_views[&origin_id].right.source_id(),
            ),
            initial_source_ids
        );

        let model = dialog.text_views.get_mut(&origin_id).unwrap();
        model.select_source_line(crate::diff::model::DiffSide::Left, 1, false);
        model
            .left
            .replace_source("changed\nselected".into())
            .unwrap();
        let reads = Cell::new(0);
        assert!(
            dialog
                .compare_selection_with_clipboard_using(origin_id, || {
                    reads.set(reads.get() + 1);
                    Ok("clipboard".into())
                })
                .unwrap_err()
                .contains("Source selection is stale")
        );
        assert_eq!(reads.get(), 0, "stale selection must not read clipboard");

        dialog
            .text_views
            .get_mut(&origin_id)
            .unwrap()
            .select_source_line(crate::diff::model::DiffSide::Left, 1, false);
        let accepted_source = dialog.text_views[&origin_id].left.source().to_owned();
        let before_navigation = dialog.workspace.navigation_stack.clone();
        let reads = Cell::new(0);
        assert!(
            dialog
                .compare_selection_with_clipboard_using(origin_id, || {
                    reads.set(reads.get() + 1);
                    Err("clipboard has no plain text".into())
                })
                .unwrap_err()
                .contains("clipboard has no plain text")
        );
        assert_eq!(reads.get(), 1);

        let oversized = "x".repeat(crate::diff::model::MEMORY_TEXT_MAX_BYTES + 1);
        let reads = Cell::new(0);
        assert!(
            dialog
                .compare_selection_with_clipboard_using(origin_id, || {
                    reads.set(reads.get() + 1);
                    Ok(oversized)
                })
                .unwrap_err()
                .contains("per-side limit")
        );
        assert_eq!(reads.get(), 1);
        assert_eq!(dialog.workspace.current_view.id, origin_id);
        assert_eq!(dialog.workspace.navigation_stack, before_navigation);
        assert_eq!(dialog.text_views[&origin_id].left.source(), accepted_source);

        dialog
            .compare_selection_with_clipboard_using(origin_id, || Ok(String::new()))
            .unwrap();
        let empty_comparison = dialog.text_views[&dialog.workspace.current_view.id]
            .right
            .source();
        assert_eq!(empty_comparison, "");
        assert_eq!(
            dialog.workspace.status.as_deref(),
            Some("Selected text compared with empty clipboard text; empty text is valid")
        );
    }

    #[test]
    fn m3c_file_folder_child_selection_compares_once_and_back_restores_file_state() {
        use std::cell::Cell;

        let left_root = tempfile::tempdir().unwrap();
        let right_root = tempfile::tempdir().unwrap();
        let left_path = left_root.path().join("source.txt");
        let right_path = right_root.path().join("source.txt");
        std::fs::write(&left_path, "before\r\nselected\r\nlast\r\n").unwrap();
        std::fs::write(&right_path, "right side\r\n").unwrap();

        let mut dialog = DiffDialogState::default();
        dialog
            .workspace
            .open_paths(
                left_root.path().display().to_string(),
                right_root.path().display().to_string(),
            )
            .unwrap();
        let folder_id = dialog.workspace.current_view.id;
        dialog
            .workspace
            .push_file_compare(
                std::path::PathBuf::from("source.txt"),
                Some(left_path.clone()),
                Some(right_path.clone()),
            )
            .unwrap();
        let origin_id = dialog.workspace.current_view.id;
        let DiffView::TextCompare(state) = &dialog.workspace.current_view.view else {
            panic!("folder child is a text comparison")
        };
        assert_eq!(state.origin, TextCompareOrigin::FileBacked);
        assert_eq!(
            state.relative_path.as_deref(),
            Some(std::path::Path::new("source.txt"))
        );
        let mut model =
            crate::diff::model::TextViewModel::load(state, &dialog.workspace.settings).unwrap();
        configure_text_model(&mut model, &dialog.workspace.settings, &dialog.persistence);
        assert_eq!(model.left.source(), "before\r\nselected\r\nlast\r\n");
        assert!(model.select_source_line(crate::diff::model::DiffSide::Left, 1, false));
        let source_id = model.left.source_id();
        let source_revision = model.left.revision;
        let original_recents = dialog.persistence.recent_comparisons.clone();
        dialog.text_views.insert(origin_id, model);

        let reads = Cell::new(0);
        dialog
            .compare_selection_with_clipboard_using(origin_id, || {
                reads.set(reads.get() + 1);
                Ok("clipboard text".into())
            })
            .unwrap();
        assert_eq!(reads.get(), 1);
        let memory_id = dialog.workspace.current_view.id;
        assert_eq!(dialog.text_views[&memory_id].left.source(), "selected\r\n");
        assert_eq!(
            dialog.text_views[&memory_id].right.source(),
            "clipboard text"
        );
        assert_eq!(dialog.persistence.recent_comparisons, original_recents);
        assert_eq!(
            std::fs::read_to_string(&left_path).unwrap(),
            "before\r\nselected\r\nlast\r\n"
        );

        assert!(dialog.navigate_back());
        assert_eq!(dialog.workspace.current_view.id, origin_id);
        let restored = &dialog.text_views[&origin_id];
        assert_eq!(restored.origin, TextCompareOrigin::FileBacked);
        assert_eq!(restored.left_path.as_deref(), Some(left_path.as_path()));
        assert_eq!(restored.left.source_id(), source_id);
        assert_eq!(restored.left.revision, source_revision);
        assert_eq!(restored.left.source(), "before\r\nselected\r\nlast\r\n");
        assert!(dialog.navigate_back());
        assert_eq!(dialog.workspace.current_view.id, folder_id);
    }

    #[test]
    fn m3c_projection_preferences_round_trip_without_source_annotations() {
        use crate::diff::text_compare::RowProjectionMode;

        let directory = tempfile::tempdir().unwrap();
        let left_path = directory.path().join("left.txt");
        let right_path = directory.path().join("right.txt");
        std::fs::write(&left_path, "same\nleft").unwrap();
        std::fs::write(&right_path, "same\nright").unwrap();
        let left = left_path.display().to_string();
        let right = right_path.display().to_string();

        let mut dialog = DiffDialogState::default();
        dialog.open_and_record(left, right).unwrap();
        let original_id = dialog.workspace.current_view.id;
        let DiffView::TextCompare(state) = &dialog.workspace.current_view.view else {
            panic!("text comparison opened")
        };
        let mut model =
            crate::diff::model::TextViewModel::load(state, &dialog.workspace.settings).unwrap();
        model.text_details_open = true;
        model.visible_whitespace = true;
        model.scroll.set_sync(false, true);
        model.set_row_projection(RowProjectionMode::IdenticalOnly, 7);
        dialog.text_views.insert(original_id, model);

        let session = dialog
            .snapshot_session("projection preferences".into())
            .unwrap();
        assert_eq!(session.projection_mode, 2);
        assert_eq!(session.projection_context, 7);
        let serialized = serde_json::to_string(&session).unwrap();
        assert!(!serialized.contains("bookmarks"));
        assert!(!serialized.contains("source_line_selection"));
        assert!(!serialized.contains("same\\nleft"));

        dialog.reopen_saved_session(&session).unwrap();
        let reopened_id = dialog.workspace.current_view.id;
        let preferences = dialog
            .restored_text_preferences
            .remove(&reopened_id)
            .unwrap();
        assert_eq!(preferences, (true, true, false, true, 2, 7));
        let mut restored = crate::diff::model::TextViewModel::new_memory(
            "one".into(),
            "two".into(),
            &Default::default(),
        )
        .unwrap();
        apply_restored_text_preferences(&mut restored, preferences);
        assert_eq!(restored.projection_mode, RowProjectionMode::IdenticalOnly);
        assert_eq!(restored.projection_context, 7);
        assert!(restored.text_details_open && restored.visible_whitespace);
    }

    #[test]
    fn memory_text_edit_inputs_apply_only_after_validation_and_keep_memory_origin() {
        let mut dialog = DiffDialogState::default();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "before left".into();
        draft.right = "before right".into();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let view_id = dialog.workspace.current_view.id;
        let model = dialog.text_views.get(&view_id).unwrap();
        let left_revision = model.left.revision;
        let left_source = model.left.source().to_owned();

        dialog.begin_text_input(TextInputPurpose::EditInputs { view_id });
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "after left\r\n".into();
        draft.right = "after right".into();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let model = dialog.text_views.get(&view_id).unwrap();
        assert_eq!(model.origin, TextCompareOrigin::Memory);
        assert_eq!(model.left.source(), "after left\r\n");
        assert_eq!(model.left.revision, left_revision + 1);
        assert!(model.left.is_dirty());

        let before_cancel = model.left.source().to_owned();
        dialog.begin_text_input(TextInputPurpose::EditInputs { view_id });
        dialog.text_input = None;
        assert_eq!(
            dialog.text_views.get(&view_id).unwrap().left.source(),
            before_cancel
        );
        assert_ne!(before_cancel, left_source);
    }

    #[test]
    fn memory_text_edit_inputs_reject_stale_revision_and_view_drafts() {
        let mut dialog = DiffDialogState::default();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "initial left".into();
        draft.right = "initial right".into();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let view_id = dialog.workspace.current_view.id;

        dialog.begin_text_input(TextInputPurpose::EditInputs { view_id });
        let mut stale_revision_draft = dialog.text_input.take().unwrap();
        stale_revision_draft.left = "stale draft text".into();
        let model = dialog.text_views.get_mut(&view_id).unwrap();
        model
            .left
            .replace_source("newer accepted text".into())
            .unwrap();
        let latest_revision = model.left.revision;
        let latest_source = model.left.source().to_owned();
        let (stale_revision_draft, error) =
            dialog.apply_text_input(stale_revision_draft).unwrap_err();
        assert!(error.contains("changed while Edit Inputs was open"));
        assert_eq!(stale_revision_draft.left, "stale draft text");
        let model = dialog.text_views.get(&view_id).unwrap();
        assert_eq!(model.left.source(), latest_source);
        assert_eq!(model.left.revision, latest_revision);

        dialog.begin_text_input(TextInputPurpose::EditInputs { view_id });
        let mut stale_view_draft = dialog.text_input.take().unwrap();
        stale_view_draft.left = "draft for previous view".into();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut other_view = dialog.text_input.take().unwrap();
        other_view.left = "other view left".into();
        other_view.right = "other view right".into();
        dialog
            .apply_text_input(other_view)
            .map_err(|(_, error)| error)
            .unwrap();
        let current_view_id = dialog.workspace.current_view.id;
        assert_ne!(current_view_id, view_id);

        let (stale_view_draft, error) = dialog.apply_text_input(stale_view_draft).unwrap_err();
        assert!(error.contains("active comparison changed"));
        assert_eq!(stale_view_draft.left, "draft for previous view");
        assert_eq!(
            dialog.text_views.get(&view_id).unwrap().left.source(),
            latest_source
        );
        assert_eq!(dialog.workspace.current_view.id, current_view_id);
    }

    #[test]
    fn memory_text_edit_inputs_reject_equal_revision_swapped_sources() {
        let mut dialog = DiffDialogState::default();
        dialog.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = dialog.text_input.take().unwrap();
        draft.left = "initial left".into();
        draft.right = "initial right".into();
        dialog
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        let view_id = dialog.workspace.current_view.id;

        let model = dialog.text_views.get_mut(&view_id).unwrap();
        model.left.replace_source("edited left".into()).unwrap();
        model.right.replace_source("edited right".into()).unwrap();
        assert_eq!(model.left.revision, model.right.revision);
        assert!(model.left.can_undo());
        assert!(model.right.can_undo());
        let original_left_source_id = model.left.source_id();
        let original_right_source_id = model.right.source_id();

        dialog.begin_text_input(TextInputPurpose::EditInputs { view_id });
        let mut stale_draft = dialog.text_input.take().unwrap();
        stale_draft.left = "draft left".into();
        stale_draft.right = "draft right".into();

        dialog.swap_sides();
        let model = dialog.text_views.get(&view_id).unwrap();
        assert_eq!(model.left.source_id(), original_right_source_id);
        assert_eq!(model.right.source_id(), original_left_source_id);
        assert_eq!(model.left.revision, model.right.revision);
        assert_eq!(model.left.source(), "edited right");
        assert_eq!(model.right.source(), "edited left");

        let (stale_draft, error) = dialog.apply_text_input(stale_draft).unwrap_err();
        assert!(error.contains("sources changed while Edit Inputs was open"));
        assert_eq!(stale_draft.left, "draft left");
        assert_eq!(stale_draft.right, "draft right");

        let model = dialog.text_views.get_mut(&view_id).unwrap();
        assert_eq!(model.left.source_id(), original_right_source_id);
        assert_eq!(model.right.source_id(), original_left_source_id);
        assert_eq!(model.left.source(), "edited right");
        assert_eq!(model.right.source(), "edited left");
        assert!(model.left.can_undo());
        assert!(model.right.can_undo());

        model.active_side = crate::diff::model::DiffSide::Left;
        assert!(model.undo());
        assert_eq!(model.left.source(), "initial right");
        assert_eq!(model.right.source(), "edited left");
    }

    #[test]
    fn memory_text_dirty_replacement_routes_preserve_retained_document_and_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let left_path = directory.path().join("left.txt");
        let right_path = directory.path().join("right.txt");
        std::fs::write(&left_path, "left file").unwrap();
        std::fs::write(&right_path, "right file").unwrap();
        let left = left_path.display().to_string();
        let right = right_path.display().to_string();

        let (mut dialog, view_id) = dirty_memory_dialog();
        dialog.workspace.settings.pane_split = 0.63;
        dialog.workspace.settings.syntax_theme = "preserved theme".into();
        dialog.persistence.recent_comparisons = vec![crate::diff::persistence::DisplayPathPairV1 {
            left: "preserved recent left".into(),
            right: "preserved recent right".into(),
            mode: crate::diff::persistence::ComparisonModeV1::Text,
        }];
        dialog.persistence.replacement_rules = vec![crate::diff::settings::ReplacementRuleV1 {
            id: "preserved-rule".into(),
            pattern: "before".into(),
            replacement: "after".into(),
            enabled: true,
        }];
        dialog.persistence.unimportant_section_rules =
            vec![crate::diff::settings::UnimportantSectionRuleV1 {
                id: "preserved-section".into(),
                pattern: "section".into(),
                enabled: true,
            }];
        let current_view = dialog.workspace.current_view.clone();
        let navigation_stack = dialog.workspace.navigation_stack.clone();
        let visible_paths = (
            dialog.workspace.left_visible.clone(),
            dialog.workspace.right_visible.clone(),
        );
        let settings = dialog.workspace.settings.clone();
        let persistence = dialog.persistence.clone();
        let (left_source, left_revision, left_saved_revision, left_can_undo) = {
            let model = dialog.text_views.get(&view_id).unwrap();
            (
                model.left.source().to_owned(),
                model.left.revision,
                model.left.saved_revision,
                model.left.can_undo(),
            )
        };
        assert!(left_can_undo);

        let assert_preserved = |dialog: &DiffDialogState| {
            assert!(dialog.open, "the active comparison remains open");
            assert_eq!(dialog.workspace.current_view, current_view);
            assert_eq!(dialog.workspace.navigation_stack, navigation_stack);
            assert_eq!(
                (
                    dialog.workspace.left_visible.clone(),
                    dialog.workspace.right_visible.clone()
                ),
                visible_paths
            );
            assert_eq!(dialog.workspace.settings, settings);
            assert_eq!(dialog.persistence, persistence);
            assert_eq!(dialog.text_views.len(), 1);
            let model = dialog.text_views.get(&view_id).unwrap();
            assert_eq!(model.origin, TextCompareOrigin::Memory);
            assert_eq!(model.left.source(), left_source);
            assert_eq!(model.left.revision, left_revision);
            assert_eq!(model.left.saved_revision, left_saved_revision);
            assert_eq!(model.left.can_undo(), left_can_undo);
            assert_eq!(model.right.source(), "initial right");
        };

        let error = dialog
            .open_payload(DiffOpenPayload {
                left: Some(left.clone()),
                right: Some(right.clone()),
            })
            .unwrap_err();
        assert!(error.contains("Save As"));
        assert_eq!(dialog.workspace.error.as_deref(), Some(error.as_str()));
        assert_preserved(&dialog);

        let recent = crate::diff::persistence::DisplayPathPairV1 {
            left: left.clone(),
            right: right.clone(),
            mode: crate::diff::persistence::ComparisonModeV1::Text,
        };
        let (recent_left, recent_right) = crate::diff::persistence::reopen_recent(&recent).unwrap();
        let error = dialog
            .open_and_record(recent_left, recent_right)
            .unwrap_err();
        assert!(error.contains("Save As"));
        assert_eq!(dialog.workspace.error.as_deref(), Some(error.as_str()));
        assert_preserved(&dialog);

        let mut session = crate::diff::persistence::SavedDiffSessionV1::default();
        session.left = left;
        session.right = right;
        session.pane_split = 0.91;
        session.wrap_text = true;
        session.syntax_highlighting = false;
        session.syntax_theme = "replacement theme".into();
        session.ignore_whitespace = true;
        session.case_sensitive = false;
        session.replacement_rules = vec![crate::diff::settings::ReplacementRuleV1 {
            id: "replacement-rule".into(),
            pattern: "left".into(),
            replacement: "changed".into(),
            enabled: true,
        }];
        session.unimportant_section_rules = vec![crate::diff::settings::UnimportantSectionRuleV1 {
            id: "replacement-section".into(),
            pattern: "header".into(),
            enabled: true,
        }];
        let error = dialog.reopen_saved_session(&session).unwrap_err();
        assert!(error.contains("Save As"));
        assert_eq!(dialog.workspace.error.as_deref(), Some(error.as_str()));
        assert_preserved(&dialog);

        let model = dialog.text_views.get_mut(&view_id).unwrap();
        assert!(
            model.left.undo(),
            "the original scratch edit remains undoable"
        );
        assert_eq!(model.left.source(), "initial left");
    }

    #[test]
    fn memory_text_close_cancel_keeps_dirty_comparison_and_undo() {
        let (mut dialog, view_id) = dirty_memory_dialog();
        dialog.close_prompt = true;
        let source = dialog.text_views[&view_id].left.source().to_owned();
        let revision = dialog.text_views[&view_id].left.revision;
        let mut open_after_close_request = false;
        dialog.cancel_close_prompt(&mut open_after_close_request);
        dialog.open = open_after_close_request;

        assert!(dialog.open);
        assert!(!dialog.close_prompt);
        assert_eq!(dialog.workspace.current_view.id, view_id);
        let model = dialog.text_views.get(&view_id).unwrap();
        assert_eq!(model.left.source(), source);
        assert_eq!(model.left.revision, revision);
        assert!(model.left.is_dirty());
        assert!(model.left.can_undo());
    }

    #[test]
    fn memory_text_configured_rules_advance_revision_and_recompute() {
        let mut model =
            crate::diff::model::TextViewModel::from_test_text("a b".into(), "a  b".into());
        let initial = model.comparison.as_ref().unwrap();
        assert_eq!(initial.rules_revision, 0);
        assert!(!initial.equal_under_rules);

        let settings = crate::diff::settings::DiffConfigV1 {
            ignore_whitespace: true,
            ..Default::default()
        };
        configure_text_model(
            &mut model,
            &settings,
            &crate::diff::persistence::DiffPersistenceV1::default(),
        );
        assert_eq!(model.rules.revision, 1);
        assert!(model.recalculating);
        assert!(model.comparison.as_ref().unwrap().is_stale(
            model.left.source_id(),
            model.right.source_id(),
            model.left.revision,
            model.right.revision,
            model.rules.revision,
            model.annotation_revision,
        ));

        for _ in 0..1000 {
            model.poll();
            if model.comparison.as_ref().is_some_and(|result| {
                !result.is_stale(
                    model.left.source_id(),
                    model.right.source_id(),
                    model.left.revision,
                    model.right.revision,
                    model.rules.revision,
                    model.annotation_revision,
                )
            }) && !model.recalculating
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let result = model.comparison.as_ref().unwrap();
        assert_eq!(result.rules_revision, 1);
        assert!(result.equal_under_rules);
        assert!(!model.recalculating);
    }
}
