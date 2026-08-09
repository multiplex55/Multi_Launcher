use crate::diff::model::{DiffView, DiffWorkspace};
use crate::diff::query::DiffOpenPayload;
use eframe::egui;
use std::collections::HashMap;
use std::collections::HashSet;
mod folder_view;
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
    // Binary renderers will keep their ephemeral resources in this view-id map.
    binary_views: HashMap<u64, ()>,
    close_prompt: bool,
    pub persistence: crate::diff::persistence::DiffPersistenceV1,
}

impl DiffDialogState {
    pub fn open_payload(&mut self, payload: DiffOpenPayload) -> Result<(), String> {
        self.open = true;
        self.clear_runtime_resources();
        self.workspace = DiffWorkspace::default();
        self.workspace.open_invocation(payload.left, payload.right)
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
    }

    fn clear_runtime_resources(&mut self) {
        self.text_views.clear();
        self.folder_runtimes.clear();
        self.binary_views.clear();
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
                self.workspace.back();
                self.reconcile_runtime_resources();
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
                    let result = self.workspace.open_paths(
                        self.workspace.left_visible.clone(),
                        self.workspace.right_visible.clone(),
                    );
                    if result.is_ok() {
                        self.reconcile_runtime_resources();
                    }
                }
            });
            if let Some(error) = &self.workspace.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            if self.workspace.navigation_stack.len() > 0 && ui.button("← Back").clicked() {
                self.workspace.back();
                self.reconcile_runtime_resources();
            }
            ui.separator();
            // Copy only lightweight context before borrowing the retained view.
            let workspace_id = self.workspace.workspace_id;
            let view_id = self.workspace.current_view.id;
            let settings = self.workspace.settings.clone();
            let mut action = folder_view::FolderViewAction::Noop;
            let mut render_error = None;
            match &mut self.workspace.current_view.view {
                DiffView::Start => {
                    ui.label("Choose two files or two folders to compare.");
                }
                DiffView::TextCompare(s) => {
                    if matches!(s.kind, crate::diff::model::FileComparisonKind::Binary) {
                        ui.heading(match s.kind {
                            crate::diff::model::FileComparisonKind::Text => "Text comparison",
                            crate::diff::model::FileComparisonKind::Binary => "Binary files",
                        });
                        ui.label(format!(
                            "Left: {}",
                            s.left
                                .as_ref()
                                .map_or("(missing)".into(), |p| p.display().to_string())
                        ));
                    } else {
                        if !self.text_views.contains_key(&view_id) {
                            match crate::diff::model::TextViewModel::load(&s, &settings) {
                                Ok(m) => {
                                    self.text_views.insert(view_id, m);
                                }
                                Err(e) => {
                                    render_error = Some(e);
                                }
                            }
                        }
                        if let Some(m) = self.text_views.get_mut(&view_id) {
                            text_view::show(ui, workspace_id, view_id, m);
                        }
                    }
                    ui.label(format!(
                        "Right: {}",
                        s.right
                            .as_ref()
                            .map_or("(missing)".into(), |p| p.display().to_string())
                    ));
                }
                DiffView::FolderCompare(s) => {
                    let runtime = self.folder_runtimes.entry(view_id).or_default();
                    poll_folder_runtime(s, runtime);
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
                    action = render_retained_folder(s, |state| folder_view::show(ui, state));
                }
            }
            if let Some(error) = render_error {
                self.workspace.error = Some(error);
            }
            self.apply_folder_action(action);
        });
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
                self.workspace.back();
            }
            folder_view::FolderViewAction::RequestRescan => {
                if let Some(runtime) = self.folder_runtimes.remove(&self.workspace.current_view.id)
                {
                    runtime.cancel();
                }
            }
        }
        self.reconcile_runtime_resources();
    }
}

fn poll_folder_runtime(
    state: &mut crate::diff::model::FolderCompareState,
    runtime: &mut crate::diff::folder_runtime::FolderRuntime,
) {
    use crate::diff::folder_compare::PathKeyPolicy;
    use crate::diff::folder_scan::{RootIdentity, ScanEvent};

    let left_identity = RootIdentity::new(state.left_root.clone());
    let right_identity = RootIdentity::new(state.right_root.clone());
    if runtime.left_root.as_ref() != Some(&left_identity)
        || runtime.right_root.as_ref() != Some(&right_identity)
    {
        runtime.cancel();
        runtime.generation = crate::diff::folder_runtime::FolderRuntime::next_generation();
        runtime.left_root = Some(left_identity.clone());
        runtime.right_root = Some(right_identity.clone());
        runtime.left_visited = 0;
        runtime.right_visited = 0;
        runtime.left_error = None;
        runtime.right_error = None;
        state.left_scan_complete = false;
        state.right_scan_complete = false;
        runtime.left_scan = Some(crate::diff::folder_scan::spawn_scan(
            state.left_root.clone(),
            runtime.generation,
            state.scan_rules.clone(),
        ));
        runtime.right_scan = Some(crate::diff::folder_scan::spawn_scan(
            state.right_root.clone(),
            runtime.generation,
            state.scan_rules.clone(),
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
}
