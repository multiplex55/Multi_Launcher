use crate::diff::model::{DiffView, DiffWorkspace};
use crate::diff::query::DiffOpenPayload;
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
    error: Option<String>,
    recent_to_open: Option<crate::diff::persistence::DisplayPathPairV1>,
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
    let mut child = ui.child_ui(viewport.rect, egui::Layout::top_down(egui::Align::Min));
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

#[derive(Default)]
pub struct DiffDialogState {
    pub open: bool,
    pub workspace: DiffWorkspace,
    text_views: HashMap<u64, crate::diff::model::TextViewModel>,
    folder_runtimes: HashMap<u64, crate::diff::folder_runtime::FolderRuntime>,
    operation_preview: Option<operation_preview::OperationPreview>,
    binary_views: HashMap<u64, crate::diff::binary_compare::BinaryViewModel>,
    watch_views: HashMap<u64, crate::diff::watch::ViewWatchRuntime>,
    restored_text_preferences: HashMap<u64, (bool, bool, bool, bool, u8)>,
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
    pub fn open_payload(&mut self, payload: DiffOpenPayload) -> Result<(), String> {
        self.open = true;
        self.clear_runtime_resources();
        self.workspace = DiffWorkspace::new(self.persistence.config.clone());
        match (payload.left, payload.right) {
            (Some(left), Some(right)) => self.open_and_record(left, right),
            (left, right) => self.workspace.open_invocation(left, right),
        }
    }

    fn open_and_record(&mut self, left: String, right: String) -> Result<(), String> {
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
            projection_mode: text_model.map_or(0, |m| {
                if m.projection_mode
                    == crate::diff::text_compare::RowProjectionMode::DifferencesOnly
                {
                    1
                } else {
                    0
                }
            }),
        })
    }

    pub fn reopen_saved_session(
        &mut self,
        session: &crate::diff::persistence::SavedDiffSessionV1,
    ) -> Result<(), String> {
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
        self.text_views.retain(|id, _| retained.contains(id));
        self.folder_runtimes.retain(|id, _| retained.contains(id));
        self.binary_views.retain(|id, _| retained.contains(id));
        self.watch_views.retain(|id, _| retained.contains(id));
    }

    fn clear_runtime_resources(&mut self) {
        self.text_views.clear();
        self.folder_runtimes.clear();
        self.binary_views.clear();
        self.watch_views.clear();
    }
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let screen = ctx.input(|i| i.screen_rect());
        let (initial_size, initial_position) = crate::diff::model::validated_window_geometry(
            &self.persistence,
            [screen.left(), screen.top(), screen.right(), screen.bottom()],
        );
        let minimum_size = crate::diff::model::diff_window_min_size([
            screen.left(),
            screen.top(),
            screen.right(),
            screen.bottom(),
        ]);
        let mut window = egui::Window::new("Diff")
            .id(egui::Id::new(("diff_window", self.workspace.workspace_id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(initial_size)
            .min_size(minimum_size);
        if let Some(position) = initial_position {
            window = window.default_pos(position);
        }
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
                                        ui.close_menu();
                                    }
                                    let files = matches!(
                                        self.workspace.current_view.view,
                                        DiffView::TextCompare(_) | DiffView::BinaryCompare(_)
                                    );
                                    if ui
                                        .add_enabled(files, egui::Button::new("Reload Files"))
                                        .clicked()
                                    {
                                        self.reload_files();
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(files, egui::Button::new("Recompare"))
                                        .clicked()
                                    {
                                        self.recompare();
                                        ui.close_menu();
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
                                        ui.close_menu();
                                    }
                                });
                            },
                        );
                    });
                if let Some(error) = &self.workspace.error {
                    egui::ScrollArea::both().max_height(48.0).show(ui, |ui| {
                        ui.set_max_width(ui.available_width());
                        ui.colored_label(egui::Color32::RED, error);
                    });
                }
                if self.workspace.navigation_stack.len() > 0 && ui.button("← Back").clicked() {
                    self.navigate_back();
                }
                ui.separator();
                let outcome =
                    allocate_remaining_workspace(ui, |ui| self.render_workspace(ctx, ui)).inner;
                if let Some(error) = outcome.error {
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
        self.show_operation_preview(ctx);
        if let Some(response) = response {
            if let Some(inner_size) = response.inner {
                if let Some((size, position)) = runtime_window_geometry(
                    inner_size,
                    response.response.rect.left_top(),
                    [screen.left(), screen.top(), screen.right(), screen.bottom()],
                ) {
                    self.persistence.window_size = Some(size);
                    self.persistence.window_position = Some(position);
                }
            }
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
                                    ok &= m.save(crate::diff::model::DiffSide::Left)
                                }
                                if m.right.is_dirty() {
                                    ok &= m.save(crate::diff::model::DiffSide::Right)
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
                            self.close_prompt = false;
                            open = true;
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
    ) -> WorkspaceRenderOutcome {
        // Copy only lightweight context before borrowing the retained view.
        let workspace_id = self.workspace.workspace_id;
        let view_id = self.workspace.current_view.id;
        let settings = self.workspace.settings.clone();
        let mut action = folder_view::FolderViewAction::Noop;
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
                            m.rules.ignore_all_whitespace = settings.ignore_whitespace;
                            m.rules.case_sensitive = settings.case_sensitive;
                            m.rules.replacements = self
                                .persistence
                                .replacement_rules
                                .iter()
                                .filter(|r| r.enabled)
                                .map(|r| crate::diff::text_compare::RegexReplacement {
                                    pattern: r.pattern.clone(),
                                    replacement: r.replacement.clone(),
                                })
                                .collect();
                            m.rules.unimportant_sections = self
                                .persistence
                                .unimportant_section_rules
                                .iter()
                                .filter(|r| r.enabled)
                                .map(|r| r.pattern.clone())
                                .collect();
                            m.schedule_compare();
                            if let Some((details, whitespace, vertical, horizontal, projection)) =
                                self.restored_text_preferences.remove(&view_id)
                            {
                                m.text_details_open = details;
                                m.visible_whitespace = whitespace;
                                m.scroll.set_sync(vertical, horizontal);
                                m.projection_mode = if projection == 1 {
                                    crate::diff::text_compare::RowProjectionMode::DifferencesOnly
                                } else {
                                    crate::diff::text_compare::RowProjectionMode::All
                                };
                            }
                            self.text_views.insert(view_id, m);
                        }
                        Err(e) => {
                            render_error = Some(e);
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
                        crate::diff::watch::ViewWatchRuntime::text(
                            tag,
                            s.left.clone(),
                            s.right.clone(),
                        ),
                    );
                }
                if let Some(m) = self.text_views.get_mut(&view_id) {
                    let dirty = [m.left.is_dirty(), m.right.is_dirty()];
                    let actions = self
                        .watch_views
                        .get_mut(&view_id)
                        .map(|w| w.poll(std::time::Instant::now(), dirty))
                        .unwrap_or_default();
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
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    format!(
                                        "{label} changed on disk; in-memory edits were preserved."
                                    ),
                                );
                                if ui.button("Reload / discard edits").clicked() {
                                    if let Some(loaded) = self
                                        .watch_views
                                        .get_mut(&view_id)
                                        .and_then(|w| w.resolve_text_conflict(side, true))
                                    {
                                        let _ = m.reload_external(side, &loaded);
                                    }
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
                    text_view::show(ui, workspace_id, view_id, m);
                }
                ui.label(format!(
                    "Right: {}",
                    s.right
                        .as_ref()
                        .map_or("(missing)".into(), |p| p.display().to_string())
                ));
            }
            DiffView::BinaryCompare(s) => {
                if !self.binary_views.contains_key(&view_id) {
                    match crate::diff::binary_compare::BinaryViewModel::load(s, settings.pane_split)
                    {
                        Ok(model) => {
                            self.binary_views.insert(view_id, model);
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
                if refresh {
                    if let Some(model) = self.binary_views.get_mut(&view_id) {
                        if let Err(error) = model.refresh_external(s) {
                            render_error = Some(format!("Binary view is stale: {error}"));
                        }
                    }
                }
                if let Some(model) = self.binary_views.get_mut(&view_id) {
                    binary_view::show(ui, workspace_id, view_id, model);
                }
            }
            DiffView::FolderCompare(s) => {
                let runtime = self.folder_runtimes.entry(view_id).or_default();
                poll_folder_runtime(s, runtime);
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
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("Scan failures — left: {left}; right: {right}"),
                    );
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
                if let DiffView::FolderCompare(state) = &mut self.workspace.current_view.view {
                    if state.apply_draft().is_ok() {
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
                    view_id,
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
            if model.left.is_dirty() {
                if let Some(path) = &model.left_path {
                    paths.insert(path.clone());
                }
            }
            if model.right.is_dirty() {
                if let Some(path) = &model.right_path {
                    paths.insert(path.clone());
                }
            }
        }
        paths
    }

    fn show_operation_preview(&mut self, ctx: &egui::Context) {
        let active = self
            .operation_preview
            .as_ref()
            .and_then(|p| self.folder_runtimes.get(&p.view_id))
            .is_some_and(|runtime| runtime.mutation_active());
        let response = self.operation_preview.as_mut().map(|p| p.show(ctx, active));
        match response {
            Some(operation_preview::PreviewResponse::Cancel) => self.operation_preview = None,
            Some(operation_preview::PreviewResponse::Execute) => {
                let dirty = self.dirty_paths();
                let preview = self.operation_preview.take().expect("preview exists");
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
                        if let Some(runtime) = self.folder_runtimes.get_mut(&preview.view_id) {
                            runtime.active_operation = Some(handle);
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

fn poll_folder_runtime(
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
) {
    use crate::diff::folder_scan::RootIdentity;

    let completed = runtime.active_operation.as_ref().and_then(|operation| {
        operation.receiver.try_iter().find_map(|event| match event {
            crate::diff::file_ops::OperationEvent::Completed(report)
                if report.generation == runtime.generation =>
            {
                Some(report)
            }
            _ => None,
        })
    });
    if let Some(report) = completed {
        runtime.active_operation = None;
        refresh_after_mutation(state, runtime, report.affected_subtree);
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

    let left_identity = RootIdentity::new(state.left_root.clone());
    let right_identity = RootIdentity::new(state.right_root.clone());
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
        runtime.left_scan = Some(crate::diff::folder_scan::spawn_scan(
            state.left_root.clone(),
            runtime.generation,
            state.applied_scan_rules.clone(),
        ));
        runtime.right_scan = Some(crate::diff::folder_scan::spawn_scan(
            state.right_root.clone(),
            runtime.generation,
            state.applied_scan_rules.clone(),
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
            ui.close_menu();
        }
        if ui.button("Select Folder…").clicked() {
            workspace.assign_selected_path(side, rfd::FileDialog::new().pick_folder());
            ui.close_menu();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::model::{DiffView, FolderCompareState, RetainedView};

    fn folder_dialog(id: u64) -> DiffDialogState {
        let mut dialog = DiffDialogState::default();
        dialog.workspace.current_view = RetainedView {
            id,
            view: DiffView::FolderCompare(FolderCompareState::default()),
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
        let ctx = egui::Context::default();
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
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_max(
                        egui::pos2(screen[0], screen[1]),
                        egui::pos2(screen[2], screen[3]),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let mut window = egui::Window::new("Diff")
                        .id(egui::Id::new(("geometry_regression", workspace_id)))
                        .default_size(restored_size)
                        .min_size(crate::diff::model::diff_window_min_size(screen));
                    if let Some(position) = restored_position {
                        window = window.default_pos(position);
                    }
                    let response = window
                        .show(ctx, |ui| {
                            let viewport = DiffViewport::remaining(ui);
                            allocate_viewport(ui, viewport, |_| viewport.size).inner
                        })
                        .unwrap();
                    observation = Some((response.inner, response.response.rect));
                },
            );

            let (inner_size, outer_rect) = observation.unwrap();
            let inner_size = inner_size.expect("window body rendered");
            let geometry = runtime_window_geometry(inner_size, outer_rect.left_top(), screen)
                .expect("complete window geometry");
            assert!((geometry.0[0] - 900.0).abs() <= tolerance);
            assert!((geometry.0[1] - 650.0).abs() <= tolerance);
            assert!(
                outer_rect.width() > inner_size.x || outer_rect.height() > inner_size.y,
                "the outer response should be allowed to include window chrome"
            );
            persistence.window_size = Some(geometry.0);
            persistence.window_position = Some(geometry.1);
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
        let operation_preview::PreviewPlan::Copy(plan) = preview.plan else {
            unreachable!()
        };
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
        refresh_after_mutation(state, runtime, report.affected_subtree);
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

    fn lightweight_view(kind: usize) -> DiffView {
        match kind {
            0 => DiffView::Start,
            1 => DiffView::FolderCompare(FolderCompareState::default()),
            2 => DiffView::TextCompare(crate::diff::model::TextCompareState {
                left: None,
                right: None,
                relative_path: None,
            }),
            3 => DiffView::BinaryCompare(crate::diff::model::BinaryCompareState {
                left: None,
                right: None,
                relative_path: None,
            }),
            _ => unreachable!(),
        }
    }

    fn render_layout_frame(
        ctx: &egui::Context,
        view: &DiffView,
        folder_rows: usize,
        requested_size: egui::Vec2,
    ) -> (egui::Rect, egui::Rect, egui::Rect) {
        let mut outer = egui::Rect::NOTHING;
        let mut workspace = egui::Rect::NOTHING;
        let mut workspace_clip = egui::Rect::NOTHING;
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            requested_size + egui::vec2(200.0, 200.0),
        ));
        // egui applies a new window's default geometry through memory during
        // its first frame. Render a warm-up frame before observing geometry so
        // the assertion measures the settled, user-visible window rectangle.
        for input in [input.clone(), input] {
            let _ = ctx.run(input, |ctx| {
                let response = egui::Window::new("Diff layout regression")
                    .id(egui::Id::new("diff_layout_regression"))
                    .collapsible(false)
                    .resizable(true)
                    .default_pos(egui::pos2(40.0, 30.0))
                    .default_size(requested_size)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Left");
                            ui.label("↔");
                            ui.label("Right");
                            let _ = ui.button("Compare");
                        });
                        ui.separator();
                        allocate_remaining_workspace(ui, |ui| {
                            workspace = ui.max_rect();
                            workspace_clip = ui.clip_rect();
                            match view {
                                DiffView::Start => {
                                    ui.label("Start");
                                }
                                DiffView::FolderCompare(_) => {
                                    // Vary the amount of painted folder content without
                                    // letting the deterministic renderer request more layout
                                    // space than the bounded workspace owns.
                                    let painter = ui.painter();
                                    for row in 0..folder_rows {
                                        let y = workspace.top() + row as f32 * 4.0;
                                        painter.line_segment(
                                            [
                                                egui::pos2(workspace.left(), y),
                                                egui::pos2(workspace.right(), y),
                                            ],
                                            egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
                                        );
                                    }
                                }
                                DiffView::TextCompare(_) => {
                                    ui.label("Text");
                                }
                                DiffView::BinaryCompare(_) => {
                                    ui.label("Binary");
                                }
                            };
                        });
                    })
                    .unwrap();
                outer = response.response.rect;
            });
        }
        (outer, workspace, workspace_clip)
    }

    fn assert_rect_close(actual: egui::Rect, expected: egui::Rect) {
        let tolerance = 0.5;
        assert!((actual.min.x - expected.min.x).abs() <= tolerance);
        assert!((actual.min.y - expected.min.y).abs() <= tolerance);
        assert!((actual.max.x - expected.max.x).abs() <= tolerance);
        assert!((actual.max.y - expected.max.y).abs() <= tolerance);
    }

    #[test]
    fn workspace_views_retain_requested_window_geometry_and_parent_clip() {
        for requested in [
            egui::vec2(400.0, 250.0),
            egui::vec2(900.0, 650.0),
            egui::vec2(1600.0, 1000.0),
        ] {
            let ctx = egui::Context::default();
            let mut expected = None;
            for kind in 0..4 {
                let (outer, workspace, clip) =
                    render_layout_frame(&ctx, &lightweight_view(kind), 1, requested);
                // `Window::default_size` is the requested inner size; the outer
                // response additionally includes the platform/style frame.
                assert!(outer.width() >= requested.x, "{outer:?}");
                assert!(outer.height() >= requested.y, "{outer:?}");
                assert!(outer.contains_rect(workspace));
                assert!(outer.contains_rect(clip));
                assert!(clip.contains_rect(workspace));
                // `InnerResponse::response` describes the widgets' content and may
                // shrink to a short label. The allocated UI's max rect is the
                // layout boundary that must reserve the remaining window space.
                // The workspace spans the requested inner width. The outer rect
                // is wider because it also includes the window frame.
                assert!(
                    (workspace.width() - requested.x).abs() <= 0.5,
                    "{workspace:?}"
                );
                assert!(workspace.height() <= requested.y, "{workspace:?}");
                assert!(workspace.height() >= 0.0, "{workspace:?}");
                if let Some(expected) = expected {
                    assert_rect_close(outer, expected);
                } else {
                    expected = Some(outer);
                }
            }
        }
    }

    #[test]
    fn folder_content_amount_does_not_resize_outer_window() {
        let ctx = egui::Context::default();
        let view = lightweight_view(1);
        let size = egui::vec2(640.0, 480.0);
        let (baseline, _, _) = render_layout_frame(&ctx, &view, 1, size);
        for rows in [15, 1_000] {
            let (outer, workspace, clip) = render_layout_frame(&ctx, &view, rows, size);
            assert_rect_close(outer, baseline);
            assert!(workspace.contains_rect(clip.intersect(workspace)));
            assert!(clip.contains_rect(workspace));
        }
    }

    #[test]
    fn user_resize_grows_workspace_instead_of_using_content_size() {
        let view = lightweight_view(0);
        let (_, small, _) = render_layout_frame(
            &egui::Context::default(),
            &view,
            0,
            egui::vec2(640.0, 480.0),
        );
        let (_, large, clip) = render_layout_frame(
            &egui::Context::default(),
            &view,
            0,
            egui::vec2(1_000.0, 720.0),
        );
        assert!(large.width() > small.width());
        assert!(large.height() > small.height());
        assert!(clip.contains_rect(large));
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
}
