use crate::diff::model::{DiffView, DiffWorkspace};
use crate::diff::query::DiffOpenPayload;
use eframe::egui;
use std::collections::HashMap;
use std::collections::HashSet;
mod binary_view;
mod folder_view;
mod operation_preview;
mod rules_dialog;
mod text_view;

fn render_retained_folder(
    state: &mut crate::diff::model::FolderCompareState,
    render: impl FnOnce(&mut crate::diff::model::FolderCompareState) -> folder_view::FolderViewAction,
) -> folder_view::FolderViewAction {
    render(state)
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
    close_prompt: bool,
    pub persistence: crate::diff::persistence::DiffPersistenceV1,
}

impl DiffDialogState {
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
        if let DiffView::FolderCompare(folder) = &mut self.workspace.current_view.view {
            folder.draft_include_rules = session.folder_includes.join("\n");
            folder.draft_exclude_rules = session.folder_excludes.join("\n");
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
        let mut window = egui::Window::new("Diff")
            .id(egui::Id::new(("diff_window", self.workspace.workspace_id)))
            .open(&mut open)
            .resizable(true)
            .default_size(initial_size)
            .min_size([600.0, 350.0]);
        if let Some(position) = initial_position {
            window = window.default_pos(position);
        }
        let response = window.show(ctx, |ui| {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)) {
                self.navigate_back();
            }
            ui.horizontal(|ui| {
                let left = ui.add(
                    egui::TextEdit::singleline(&mut self.workspace.left_visible)
                        .id(egui::Id::new((self.workspace.workspace_id, "left")))
                        .hint_text("Left file or folder"),
                );
                if self.workspace.focus_left_requested {
                    left.request_focus();
                    self.workspace.focus_left_requested = false;
                }
                picker_menu(ui, &mut self.workspace, crate::diff::model::DiffSide::Left);
                ui.label("↔");
                ui.add(
                    egui::TextEdit::singleline(&mut self.workspace.right_visible)
                        .id(egui::Id::new((self.workspace.workspace_id, "right")))
                        .hint_text("Right file or folder"),
                );
                picker_menu(ui, &mut self.workspace, crate::diff::model::DiffSide::Right);
                if ui
                    .button("Compare")
                    .on_hover_text("Compare/change paths (Ctrl+O)")
                    .clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O))
                {
                    let _ = self.open_and_record(
                        self.workspace.left_visible.clone(),
                        self.workspace.right_visible.clone(),
                    );
                }
            });
            if let Some(error) = &self.workspace.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            if self.workspace.navigation_stack.len() > 0 && ui.button("← Back").clicked() {
                self.navigate_back();
            }
            ui.separator();
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
                            let label = format!("{} ↔ {} ({:?})", recent.left, recent.right, recent.mode);
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
                                m.rules.replacements = self.persistence.replacement_rules.iter().filter(|r| r.enabled).map(|r| crate::diff::text_compare::RegexReplacement { pattern: r.pattern.clone(), replacement: r.replacement.clone() }).collect();
                                m.rules.unimportant_sections = self.persistence.unimportant_section_rules.iter().filter(|r| r.enabled).map(|r| r.pattern.clone()).collect();
                                m.schedule_compare();
                                self.text_views.insert(view_id, m);
                            }
                            Err(e) => {
                                render_error = Some(e);
                            }
                        }
                    }
                    let tag = crate::diff::watch::WatchTag { workspace: workspace_id, view: view_id, generation: 1 };
                    if self.watch_views.get(&view_id).is_none_or(|watch| watch.tag != tag) {
                        self.watch_views.insert(view_id, crate::diff::watch::ViewWatchRuntime::text(tag, s.left.clone(), s.right.clone()));
                    }
                    if let Some(m) = self.text_views.get_mut(&view_id) {
                        let dirty = [m.left.is_dirty(), m.right.is_dirty()];
                        let actions = self.watch_views.get_mut(&view_id).map(|w| w.poll(std::time::Instant::now(), dirty)).unwrap_or_default();
                        for action in actions {
                            match action {
                                crate::diff::watch::ViewWatchAction::TextReload { side, loaded } => { if let Err(e) = m.reload_external(side, &loaded) { render_error = Some(e); } }
                                crate::diff::watch::ViewWatchAction::TextConflict { side, .. } => m.external_conflict[if side == crate::diff::model::DiffSide::Left { 0 } else { 1 }] = true,
                                _ => {}
                            }
                        }
                        for (index, side, label) in [(0, crate::diff::model::DiffSide::Left, "Left"), (1, crate::diff::model::DiffSide::Right, "Right")] {
                            if m.external_conflict[index] {
                                ui.horizontal(|ui| {
                                    ui.colored_label(egui::Color32::YELLOW, format!("{label} changed on disk; in-memory edits were preserved."));
                                    if ui.button("Reload / discard edits").clicked() {
                                        if let Some(loaded) = self.watch_views.get_mut(&view_id).and_then(|w| w.resolve_text_conflict(side, true)) { let _ = m.reload_external(side, &loaded); }
                                    }
                                    if ui.button("Keep current").clicked() {
                                        if let Some(w) = self.watch_views.get_mut(&view_id) { w.resolve_text_conflict(side, false); }
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
                        match crate::diff::binary_compare::BinaryViewModel::load(
                            s,
                            settings.pane_split,
                        ) {
                            Ok(model) => {
                                self.binary_views.insert(view_id, model);
                            }
                            Err(error) => {
                                render_error = Some(error);
                            }
                        }
                    }
                    let tag = crate::diff::watch::WatchTag { workspace: workspace_id, view: view_id, generation: 1 };
                    if self.watch_views.get(&view_id).is_none_or(|watch| watch.tag != tag) {
                        self.watch_views.insert(view_id, crate::diff::watch::ViewWatchRuntime::binary(tag, s.left.clone(), s.right.clone()));
                    }
                    let refresh = self.watch_views.get_mut(&view_id).is_some_and(|watch| watch.poll(std::time::Instant::now(), [false; 2]).into_iter().any(|a| matches!(a, crate::diff::watch::ViewWatchAction::BinaryRefresh)));
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
                    let tag = crate::diff::watch::WatchTag { workspace: workspace_id, view: view_id, generation: runtime.generation };
                    if self.watch_views.get(&view_id).is_none_or(|watch| watch.tag != tag) {
                        self.watch_views.insert(view_id, crate::diff::watch::ViewWatchRuntime::folder(tag, s.left_root.clone(), s.right_root.clone()));
                    }
                    let changes = self.watch_views.get_mut(&view_id).map(|w| w.poll(std::time::Instant::now(), [false; 2])).unwrap_or_default();
                    for change in changes {
                        if let crate::diff::watch::ViewWatchAction::FolderChanged { subtree, .. } = change {
                            for entry in s.model.entries.values() {
                                if entry.relative_path.starts_with(&subtree) { s.stale_paths.insert(entry.relative_path.clone()); }
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
                    action =
                        render_retained_folder(s, |state| folder_view::show(ui, state, runtime));
                }
            }
            if let Some(error) = render_error {
                self.workspace.error = Some(error);
            }
            if let Some(recent) = recent_to_open {
                match crate::diff::persistence::reopen_recent(&recent) {
                    Ok((left, right)) => { let _ = self.open_and_record(left, right); }
                    Err(error) => self.workspace.error = Some(error.to_string()),
                }
            }
            self.apply_folder_action(action);
        });
        self.show_operation_preview(ctx);
        if let Some(response) = response {
            let rect = response.response.rect;
            self.persistence.window_size = Some([rect.width(), rect.height()]);
            self.persistence.window_position = Some([rect.left(), rect.top()]);
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
                    if let Ok(rules) = state.validated_draft_scan_rules() {
                        if rules != state.applied_scan_rules {
                            state.applied_scan_rules = rules;
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
        _ => All,
    }
}

fn poll_folder_runtime(
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
) {
    use crate::diff::folder_compare::PathKeyPolicy;
    use crate::diff::folder_scan::{RootIdentity, ScanEvent};

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

    fn process_scan_events(
        state: &mut crate::diff::model::FolderCompareState,
        runtime: &mut crate::diff::folder_runtime::FolderRuntime,
        left: bool,
        expected: &RootIdentity,
        events: Vec<ScanEvent>,
    ) {
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
            state.draft_include_rules = "*.tmp".into();
            state.draft_exclude_rules = ".git/\ntarget/".into();
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
}
