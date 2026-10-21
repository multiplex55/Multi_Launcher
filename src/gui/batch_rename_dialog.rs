use crate::batch_rename::{
    self, CaseConversion, EntryInclusion, ExplicitDirectoryRoot, ExtensionPolicy, PreviewReadiness,
    PreviewRowStatus, RenameRule, RenameRuleSet, RulesRevision, SnapshotEntryKind,
    SnapshotEntryRef,
};
use crate::file_operations::{
    RenameRuntimeSnapshot, RenameScanCoordinator, RenameScanGeneration, RenameScanResult,
    RenameScanStatus, RenameScanTicket, VerifiedRenamePlan, VerifiedRenamePreview,
    VerifiedRenameSnapshot,
};
use eframe::egui::{self, Align, Layout};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

const WINDOW_ID: &str = "batch_rename_preview_dialog";
const PREVIEW_ROW_HEIGHT: f32 = 25.0;
const PATH_PREVIEW_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BatchRenameUiCommand {
    SelectRoot(PathBuf),
    Rescan,
    RecursiveChanged,
    ApplyConfirmed,
    UndoConfirmed(String),
    CancelOperation,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanBanner {
    ChooseRoot,
    Scanning,
    Complete,
    Incomplete(RenameScanStatus),
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisualSortColumn {
    ScanOrder,
    OriginalName,
    RelativePath,
    FullPath,
    ProposedPath,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleRowAction {
    None,
    MoveUp,
    MoveDown,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewRuleKind {
    Prefix,
    Suffix,
    Number,
    LiteralReplace,
    RegexReplace,
    Lowercase,
    Uppercase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VisualSort {
    column: VisualSortColumn,
    descending: bool,
}

impl Default for VisualSort {
    fn default() -> Self {
        Self {
            column: VisualSortColumn::ScanOrder,
            descending: false,
        }
    }
}

#[derive(Debug, Clone)]
struct RuleDraft {
    id: u64,
    rule: RenameRule,
}

struct PreviewDisplayRow {
    entry_ref: SnapshotEntryRef,
    relative_path: PathBuf,
    source_path: PathBuf,
    source_name: OsString,
    proposed_path: Option<PathBuf>,
    proposed_name: Option<OsString>,
    status: PreviewRowStatus,
}

#[derive(Default)]
struct FolderNode {
    path: PathBuf,
    name: OsString,
    children: Vec<FolderNode>,
}

/// Presentation and draft state for the read-only batch rename workflow.
/// Filesystem enumeration and native identity proof remain owned by
/// `RenameScanCoordinator` and `VerifiedRenameSnapshot`.
pub(crate) struct BatchRenameDialogState {
    pub open: bool,
    root_path: Option<PathBuf>,
    recursive: bool,
    scan_banner: ScanBanner,
    scan_result: Option<RenameScanResult>,
    scan_ticket: Option<RenameScanTicket>,
    scan_generation: Option<RenameScanGeneration>,
    ticket_session_epoch: u64,
    session_epoch: u64,
    snapshot: Option<VerifiedRenameSnapshot>,
    verified_preview: Option<VerifiedRenamePreview>,
    pending_intent: Option<VerifiedRenamePlan>,
    apply_confirmation_open: bool,
    undo_confirmation: Option<String>,
    operation_was_active: bool,
    rules: Vec<RuleDraft>,
    next_rule_id: u64,
    rules_revision: u64,
    extension_policy: ExtensionPolicy,
    new_rule_kind: NewRuleKind,
    selected_by_ref: HashMap<SnapshotEntryRef, bool>,
    included_by_ref: HashMap<SnapshotEntryRef, bool>,
    folder_tree: Vec<FolderNode>,
    display_rows: Vec<PreviewDisplayRow>,
    visible_rows: Vec<usize>,
    filter: String,
    visual_sort: VisualSort,
    status_message: Option<String>,
    #[cfg(test)]
    preview_recomputation_count: usize,
    #[cfg(test)]
    last_table_scroll_metrics: Option<(egui::Id, f32, f32, f32, f32)>,
    #[cfg(test)]
    last_table_rendered_rows: usize,
    #[cfg(test)]
    filter_widget_id: Option<egui::Id>,
}

impl Default for BatchRenameDialogState {
    fn default() -> Self {
        Self {
            open: false,
            root_path: None,
            recursive: false,
            scan_banner: ScanBanner::ChooseRoot,
            scan_result: None,
            scan_ticket: None,
            scan_generation: None,
            ticket_session_epoch: 0,
            session_epoch: 0,
            snapshot: None,
            verified_preview: None,
            pending_intent: None,
            apply_confirmation_open: false,
            undo_confirmation: None,
            operation_was_active: false,
            rules: Vec::new(),
            next_rule_id: 1,
            rules_revision: 0,
            extension_policy: ExtensionPolicy::Preserve,
            new_rule_kind: NewRuleKind::Prefix,
            selected_by_ref: HashMap::new(),
            included_by_ref: HashMap::new(),
            folder_tree: Vec::new(),
            display_rows: Vec::new(),
            visible_rows: Vec::new(),
            filter: String::new(),
            visual_sort: VisualSort::default(),
            status_message: None,
            #[cfg(test)]
            preview_recomputation_count: 0,
            #[cfg(test)]
            last_table_scroll_metrics: None,
            #[cfg(test)]
            last_table_rendered_rows: 0,
            #[cfg(test)]
            filter_widget_id: None,
        }
    }
}

impl BatchRenameDialogState {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn jump_mode_target_generation(&self) -> u64 {
        self.session_epoch
    }

    pub(crate) fn jump_mode_parent_targets_allowed(&self) -> bool {
        self.open
            && !self.operation_was_active
            && self.pending_intent.is_none()
            && !self.apply_confirmation_open
            && self.undo_confirmation.is_none()
    }

    pub(crate) fn end_session(&mut self) {
        let had_active_session = self.open || self.scan_ticket.is_some();
        self.open = false;
        if had_active_session {
            self.session_epoch = self.session_epoch.wrapping_add(1);
        }
        if let Some(ticket) = &self.scan_ticket {
            ticket.cancel();
            self.scan_banner = ScanBanner::Cancelled;
        } else {
            self.scan_banner = ScanBanner::ChooseRoot;
        }
        self.clear_snapshot_state();
        self.undo_confirmation = None;
        self.status_message = None;
    }

    pub(crate) fn observe_runtime(&mut self, runtime: &RenameRuntimeSnapshot) {
        if runtime.operation_active {
            self.operation_was_active = true;
            return;
        }
        if self.operation_was_active {
            self.operation_was_active = false;
            self.clear_snapshot_state();
            self.scan_banner = ScanBanner::Cancelled;
            self.status_message = Some(format!(
                "{}. Refresh the preview before applying another batch.",
                runtime
                    .last_message
                    .as_deref()
                    .unwrap_or("The rename operation ended")
            ));
        }
    }

    pub(crate) fn has_pending_scan(&self) -> bool {
        self.scan_ticket.is_some()
    }

    pub(crate) fn discard_finished_ticket(&mut self) {
        self.scan_ticket = None;
        self.clear_snapshot_state();
    }

    pub(crate) fn root_path(&self) -> Option<&Path> {
        self.root_path.as_deref()
    }

    pub(crate) fn start_scan(&mut self, root_path: PathBuf, coordinator: &RenameScanCoordinator) {
        self.prepare_explicit_root_scan(root_path.clone());
        let root = match ExplicitDirectoryRoot::new(root_path) {
            Ok(root) => root,
            Err(error) => {
                self.scan_banner = ScanBanner::Error;
                self.status_message = Some(format!("That directory cannot be used: {error:?}"));
                return;
            }
        };
        match coordinator.submit(root, self.recursive) {
            Ok(ticket) => {
                self.scan_generation = Some(ticket.generation());
                self.ticket_session_epoch = self.session_epoch;
                self.scan_ticket = Some(ticket);
                self.scan_banner = ScanBanner::Scanning;
                self.status_message = None;
            }
            Err(error) => {
                self.scan_banner = ScanBanner::Error;
                self.status_message = Some(format!("Could not start the directory scan: {error}"));
            }
        }
    }

    pub(crate) fn fail_to_start_scan(&mut self, root_path: PathBuf, message: String) {
        self.invalidate_for_new_scan();
        self.root_path = Some(root_path);
        self.scan_banner = ScanBanner::Error;
        self.status_message = Some(message);
    }

    pub(crate) fn poll_scan(&mut self) -> bool {
        let received = match self.scan_ticket.as_ref() {
            Some(ticket) => match ticket.try_recv() {
                Ok(Some(result)) => Some(Ok(result)),
                Ok(None) => None,
                Err(error) => Some(Err(error.to_string())),
            },
            None => None,
        };
        let Some(received) = received else {
            return false;
        };
        self.scan_ticket = None;
        let expected_generation = self.scan_generation;
        let current_session = self.open && self.ticket_session_epoch == self.session_epoch;
        match received {
            Ok(result) => {
                if !current_session || Some(result.generation()) != expected_generation {
                    if current_session {
                        self.scan_banner = ScanBanner::Cancelled;
                    }
                    return true;
                }
                self.scan_result = Some(result.clone());
                match result.status() {
                    RenameScanStatus::Complete => {
                        if let Some(generation) = expected_generation {
                            match result.accept(generation) {
                                Ok(snapshot) => {
                                    self.root_path = Some(snapshot.canonical_root().to_path_buf());
                                    self.snapshot = Some(snapshot);
                                    if let Some(result) = &self.scan_result {
                                        for row in result.rows() {
                                            self.selected_by_ref.insert(row.entry_ref(), false);
                                            self.included_by_ref.insert(row.entry_ref(), true);
                                        }
                                    }
                                    self.folder_tree = self
                                        .scan_result
                                        .as_ref()
                                        .map(build_folder_tree)
                                        .unwrap_or_default();
                                    self.scan_banner = ScanBanner::Complete;
                                    self.status_message = None;
                                    self.rebuild_preview();
                                }
                                Err(error) => {
                                    self.scan_banner = ScanBanner::Error;
                                    self.status_message = Some(format!(
                                        "The scan result was not accepted: {error:?}"
                                    ));
                                }
                            }
                        }
                    }
                    status => {
                        self.scan_banner = ScanBanner::Incomplete(status);
                        self.status_message = None;
                    }
                }
            }
            Err(error) => {
                if current_session {
                    self.scan_banner = ScanBanner::Error;
                    self.status_message = Some(format!("The scan worker stopped: {error}"));
                }
            }
        }
        true
    }

    /// Freeze the exact verified plan from the current cached preview. M8-D
    /// calls this only after its user confirmation step; M8-C exposes no
    /// mutation action.
    pub(crate) fn freeze_confirmed_plan(&mut self) -> Option<&VerifiedRenamePlan> {
        self.pending_intent = self
            .verified_preview
            .as_ref()
            .filter(|preview| preview.preview().readiness() == PreviewReadiness::Ready)
            .and_then(VerifiedRenamePreview::verified_plan)
            .filter(|plan| !plan.plan().actions().is_empty());
        self.pending_intent.as_ref()
    }

    pub(crate) fn pending_intent(&self) -> Option<&VerifiedRenamePlan> {
        self.pending_intent.as_ref()
    }

    pub(crate) fn take_pending_intent(&mut self) -> Option<VerifiedRenamePlan> {
        self.pending_intent.take()
    }

    pub(crate) fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
    }

    pub(crate) fn mark_operation_submitted(&mut self) {
        self.operation_was_active = true;
    }

    #[cfg(test)]
    pub(crate) fn has_accepted_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    #[cfg(test)]
    pub(crate) fn has_displayed_scan_result(&self) -> bool {
        self.scan_result.is_some()
    }

    pub(crate) fn set_selected(&mut self, entry_ref: SnapshotEntryRef, selected: bool) -> bool {
        self.apply_row_updates(&[(entry_ref, Some(selected), None)])
    }

    fn invalidate_for_new_scan(&mut self) {
        if let Some(ticket) = self.scan_ticket.take() {
            ticket.cancel();
        }
        self.scan_generation = None;
        self.scan_result = None;
        self.clear_snapshot_state();
        self.scan_banner = ScanBanner::Scanning;
        self.status_message = None;
    }

    fn prepare_explicit_root_scan(&mut self, root_path: PathBuf) {
        self.invalidate_for_new_scan();
        self.root_path = Some(root_path);
    }

    fn clear_snapshot_state(&mut self) {
        self.scan_result = None;
        self.scan_generation = None;
        self.snapshot = None;
        self.verified_preview = None;
        self.pending_intent = None;
        self.apply_confirmation_open = false;
        self.selected_by_ref.clear();
        self.included_by_ref.clear();
        self.folder_tree.clear();
        self.display_rows.clear();
        self.visible_rows.clear();
    }

    fn build_rules(&self) -> RenameRuleSet {
        RenameRuleSet::new(
            self.extension_policy,
            self.rules.iter().map(|draft| draft.rule.clone()).collect(),
        )
        .with_revision(RulesRevision(self.rules_revision))
    }

    fn rebuild_preview(&mut self) {
        self.pending_intent = None;
        self.display_rows.clear();
        let Some(snapshot) = &self.snapshot else {
            self.verified_preview = None;
            self.visible_rows.clear();
            return;
        };
        let preview = snapshot.preview(&self.build_rules());
        let relative_by_ref: HashMap<_, _> = self
            .scan_result
            .as_ref()
            .into_iter()
            .flat_map(|result| result.rows())
            .filter(|row| row.kind() == SnapshotEntryKind::File)
            .map(|row| (row.entry_ref(), row.relative_path().to_path_buf()))
            .collect();
        self.display_rows = preview
            .preview()
            .rows()
            .iter()
            .filter_map(|row| {
                let relative_path = relative_by_ref.get(&row.entry_ref())?.clone();
                Some(PreviewDisplayRow {
                    entry_ref: row.entry_ref(),
                    relative_path,
                    source_path: row.source_path().to_path_buf(),
                    source_name: row.source_name().to_os_string(),
                    proposed_path: row.proposed_path().map(Path::to_path_buf),
                    proposed_name: row.proposed_name().map(OsString::from),
                    status: row.status().clone(),
                })
            })
            .collect();
        self.verified_preview = Some(preview);
        self.rebuild_visible_rows();
        #[cfg(test)]
        {
            self.preview_recomputation_count = self.preview_recomputation_count.saturating_add(1);
        }
    }

    fn rebuild_visible_rows(&mut self) {
        let needle = self.filter.trim().to_lowercase();
        self.visible_rows = self
            .display_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                if needle.is_empty() {
                    return Some(index);
                }
                let matches = row
                    .source_name
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&needle)
                    || row
                        .relative_path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle)
                    || row
                        .source_path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle);
                matches.then_some(index)
            })
            .collect();
        let sort = self.visual_sort;
        self.visible_rows.sort_unstable_by(|left, right| {
            let a = &self.display_rows[*left];
            let b = &self.display_rows[*right];
            let ordering = match sort.column {
                VisualSortColumn::ScanOrder => left.cmp(right),
                VisualSortColumn::OriginalName => a
                    .source_name
                    .cmp(&b.source_name)
                    .then_with(|| a.source_path.cmp(&b.source_path)),
                VisualSortColumn::RelativePath => a
                    .relative_path
                    .cmp(&b.relative_path)
                    .then_with(|| a.source_path.cmp(&b.source_path)),
                VisualSortColumn::FullPath => a.source_path.cmp(&b.source_path),
                VisualSortColumn::ProposedPath => a
                    .proposed_path
                    .cmp(&b.proposed_path)
                    .then_with(|| a.source_path.cmp(&b.source_path)),
                VisualSortColumn::Status => status_sort_key(&a.status)
                    .cmp(&status_sort_key(&b.status))
                    .then_with(|| a.source_path.cmp(&b.source_path)),
            };
            if sort.descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }

    fn apply_row_updates(
        &mut self,
        updates: &[(SnapshotEntryRef, Option<bool>, Option<bool>)],
    ) -> bool {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return false;
        };
        let mut changed_by_ref: HashMap<SnapshotEntryRef, (Option<bool>, Option<EntryInclusion>)> =
            HashMap::with_capacity(updates.len());
        for (entry_ref, selected, included) in updates {
            if let Some(selected) = selected
                && self.selected_by_ref.get(entry_ref).copied() != Some(*selected)
            {
                changed_by_ref.entry(*entry_ref).or_default().0 = Some(*selected);
            }
            if let Some(included) = included
                && self.included_by_ref.get(entry_ref).copied() != Some(*included)
            {
                let inclusion = if *included {
                    EntryInclusion::Included
                } else {
                    EntryInclusion::Excluded
                };
                changed_by_ref.entry(*entry_ref).or_default().1 = Some(inclusion);
            }
        }
        if changed_by_ref.is_empty() {
            return false;
        }

        let domain_updates = changed_by_ref
            .iter()
            .map(|(entry_ref, (selected, inclusion))| (*entry_ref, *selected, *inclusion))
            .collect::<Vec<_>>();
        let updated = match snapshot.with_entry_updates(&domain_updates) {
            Ok(updated) => updated,
            Err(error) => {
                let message = match &error {
                    batch_rename::SnapshotError::TooManySelected { count, limit } => {
                        format!(
                            "Selection limit reached: {count} files requested; the limit is {limit}."
                        )
                    }
                    _ => format!("The file selection could not be updated: {error:?}"),
                };
                self.status_message = Some(message);
                return false;
            }
        };
        for (entry_ref, (selected, inclusion)) in changed_by_ref {
            if let Some(selected) = selected {
                self.selected_by_ref.insert(entry_ref, selected);
            }
            if let Some(inclusion) = inclusion {
                self.included_by_ref
                    .insert(entry_ref, inclusion == EntryInclusion::Included);
            }
        }
        self.snapshot = Some(updated);
        self.status_message = None;
        self.rebuild_preview();
        true
    }

    fn select_visible(&mut self, selected: bool) {
        let updates: Vec<_> = self
            .visible_rows
            .iter()
            .filter_map(|index| self.display_rows.get(*index))
            .map(|row| (row.entry_ref, Some(selected), None))
            .collect();
        self.apply_row_updates(&updates);
    }

    #[cfg(test)]
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        allow_mutations: bool,
        runtime: &RenameRuntimeSnapshot,
    ) -> Vec<BatchRenameUiCommand> {
        self.ui_with_jump_mode(ctx, None, true, allow_mutations, runtime)
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        first_ui_pass: bool,
        runtime: &RenameRuntimeSnapshot,
    ) -> Vec<BatchRenameUiCommand> {
        let mut commands = Vec::new();
        if !self.open {
            return commands;
        }
        let native_window_pass = native_ui_owner && first_ui_pass && ctx.current_pass_index() == 0;
        let mut visual_order = 0;

        let mut window_open = self.open;
        egui::Window::new("Batch Renamer")
            .id(egui::Id::new(WINDOW_ID))
            .open(&mut window_open)
            .resizable(true)
            .default_size(egui::vec2(1040.0, 760.0))
            .min_size(egui::vec2(520.0, 360.0))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.add_enabled_ui(native_ui_owner, |ui| {
                    // Window sizing/invisible passes expose a disabled content
                    // UI. Only the first visible pass may commit ordinary
                    // planning edits; the Close path retains its pre-existing
                    // availability while a worker is active.
                    let native_visible = native_window_pass
                        && ui.is_visible()
                        && !ui.is_sizing_pass()
                        && ui.is_enabled();
                    let process_interactions = native_visible
                        && !runtime.operation_active
                        && !self.apply_confirmation_open
                        && self.undo_confirmation.is_none();
                    self.draw_header(
                        ui,
                        &mut commands,
                        process_interactions,
                        scope,
                        &mut visual_order,
                    );
                    ui.separator();
                    self.draw_rule_editor(ui, process_interactions, scope, &mut visual_order);
                    ui.separator();
                    self.draw_preview(
                        ui,
                        &mut commands,
                        process_interactions,
                        scope,
                        &mut visual_order,
                    );
                    ui.separator();
                    self.draw_operation_footer(
                        ui,
                        runtime,
                        &mut commands,
                        process_interactions,
                        native_visible,
                    );
                    ui.horizontal(|ui| {
                        let close = ui.button("Close");
                        register_batch_target(
                            scope,
                            ui,
                            &close,
                            "close",
                            "Close Batch Renamer",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        );
                        if close.clicked() && native_visible {
                            commands.push(BatchRenameUiCommand::Closed);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let preview = ui
                                .add_enabled(self.snapshot.is_some(), egui::Button::new("Preview"));
                            register_batch_target(
                                scope,
                                ui,
                                &preview,
                                "preview",
                                "Preview rename plan",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            if preview.clicked() && process_interactions {
                                self.rebuild_preview();
                            }
                        });
                    });
                });
            });

        if native_window_pass && (!window_open || commands.contains(&BatchRenameUiCommand::Closed))
        {
            self.end_session();
            if !commands.contains(&BatchRenameUiCommand::Closed) {
                commands.push(BatchRenameUiCommand::Closed);
            }
            return commands;
        }

        // These existing native confirmation windows remain the owner of
        // their own interaction. The parent scope is withheld by the host
        // while either confirmation is pending.
        self.draw_confirmations(ctx, runtime, &mut commands, first_ui_pass);
        commands
    }

    fn draw_operation_footer(
        &mut self,
        ui: &mut egui::Ui,
        runtime: &RenameRuntimeSnapshot,
        commands: &mut Vec<BatchRenameUiCommand>,
        process_interactions: bool,
        native_visible: bool,
    ) {
        if !runtime.journal_ready {
            if let Some(error) = &runtime.journal_error {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Recovery data is unavailable: {error}"),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking the batch rename recovery journal…");
                });
            }
        }
        if runtime.operation_active {
            if let Some(progress) = &runtime.progress {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "{:?}: {} ({}/{})",
                        progress.phase, progress.message, progress.completed, progress.total
                    ));
                    if ui.button("Cancel safely").clicked() && native_visible {
                        // Cancellation is a worker-owned request handled between
                        // native atomic moves, never a frame-thread filesystem action.
                        commands.push(BatchRenameUiCommand::CancelOperation);
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Rechecking the confirmed rename plan…");
                    if ui.button("Cancel safely").clicked() && native_visible {
                        commands.push(BatchRenameUiCommand::CancelOperation);
                    }
                });
            }
        } else if let Some(message) = &self.status_message {
            ui.label(message);
        } else if let Some(message) = &runtime.last_message {
            ui.label(message);
        }

        let can_apply = process_interactions
            && runtime.journal_ready
            && !runtime
                .receipts
                .iter()
                .any(|receipt| receipt.status().blocks_new_apply())
            && self.verified_preview.as_ref().is_some_and(|preview| {
                preview.preview().readiness() == PreviewReadiness::Ready
                    && preview
                        .preview()
                        .plan()
                        .is_some_and(|plan| !plan.actions().is_empty())
            });
        if ui
            .add_enabled(can_apply, egui::Button::new("Apply selected renames…"))
            .clicked()
            && self.freeze_confirmed_plan().is_some()
        {
            self.apply_confirmation_open = true;
        }

        if runtime.receipts.is_empty() {
            return;
        }
        ui.collapsing("Rename recovery and Undo", |ui| {
            for receipt in runtime.receipts.iter().rev() {
                ui.group(|ui| {
                    ui.label(format!("{} · {:?}", receipt.id(), receipt.status()));
                    ui.label(receipt.root_path().display().to_string());
                    ui.label(receipt.message());
                    let remaining = receipt.remaining_undo_count();
                    ui.horizontal(|ui| {
                        ui.label(format!("Undo entries: {remaining}"));
                        let enabled =
                            process_interactions && receipt.status().can_undo() && remaining > 0;
                        if ui
                            .add_enabled(enabled, egui::Button::new("Undo…"))
                            .clicked()
                        {
                            self.undo_confirmation = Some(receipt.id().to_owned());
                        }
                    });
                    for entry in receipt.entries() {
                        ui.label(format!(
                            "{} → {} ({:?})",
                            entry.current_path().unwrap_or(entry.final_path()).display(),
                            entry.original_path().display(),
                            entry.outcome()
                        ));
                    }
                });
            }
        });
    }

    fn draw_confirmations(
        &mut self,
        ctx: &egui::Context,
        runtime: &RenameRuntimeSnapshot,
        commands: &mut Vec<BatchRenameUiCommand>,
        allow_mutations: bool,
    ) {
        let process_interactions = allow_mutations && ctx.current_pass_index() == 0;
        if self.apply_confirmation_open {
            let Some(plan) = self.pending_intent.as_ref() else {
                self.apply_confirmation_open = false;
                return;
            };
            let mut confirmation_open = true;
            let mut confirm_clicked = false;
            let mut cancel_clicked = false;
            egui::Window::new("Confirm batch rename")
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(680.0, 480.0))
                .open(&mut confirmation_open)
                .show(ctx, |ui| {
                    ui.label(format!("Root: {}", plan.canonical_root().display()));
                    ui.label(format!(
                        "Scope: {} · files: {} · scan generation: {} · rules revision: {}",
                        if plan.recursive_scope() { "recursive" } else { "direct children" },
                        plan.plan().actions().len(),
                        plan.generation().0,
                        plan.plan().rules_revision().0
                    ));
                    ui.label("These exact source and target names are frozen for this transaction.");
                    ui.label("Each file is staged in its own folder; existing names are never replaced. If a safe rollback is blocked, recovery paths remain in the receipt.");
                    ui.label("Undo checks the recorded file identity, bytes, size, and last-modified time. Content verification allows up to 1 GiB per file and 4 GiB per batch.");
                    egui::ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                        for action in plan.plan().actions() {
                            ui.label(format!(
                                "{} → {}",
                                action.source_path().display(),
                                action.target_path().display()
                            ));
                        }
                    });
                    ui.horizontal(|ui| {
                        let confirm = ui
                            .add_enabled(
                                process_interactions
                                    && runtime.journal_ready
                                    && !runtime.operation_active,
                                egui::Button::new("Confirm and apply"),
                            )
                            .clicked();
                        confirm_clicked = confirm;
                        cancel_clicked = ui.button("Cancel").clicked();
                    });
                });
            if confirm_clicked {
                commands.push(BatchRenameUiCommand::ApplyConfirmed);
                self.apply_confirmation_open = false;
            } else if cancel_clicked || !confirmation_open {
                self.cancel_apply_confirmation();
            }
        }
        if let Some(transaction_id) = self.undo_confirmation.clone() {
            if let Some(receipt) = runtime
                .receipts
                .iter()
                .find(|receipt| receipt.id() == transaction_id)
            {
                let mut confirmation_open = true;
                egui::Window::new("Confirm Undo")
                    .collapsible(false)
                    .resizable(true)
                    .default_size(egui::vec2(680.0, 420.0))
                    .open(&mut confirmation_open)
                    .show(ctx, |ui| {
                        ui.label(format!("Root: {}", receipt.root_path().display()));
                        ui.label(format!(
                            "Restore {} recorded file(s) to their original names and bytes.",
                            receipt.remaining_undo_count()
                        ));
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for entry in receipt.entries() {
                                    if let Some(current) = entry.current_path()
                                        && current != entry.original_path()
                                    {
                                        ui.label(format!(
                                            "{} → {}",
                                            current.display(),
                                            entry.original_path().display()
                                        ));
                                    }
                                }
                            });
                        ui.horizontal(|ui| {
                            let confirm = ui
                                .add_enabled(
                                    process_interactions
                                        && runtime.journal_ready
                                        && !runtime.operation_active,
                                    egui::Button::new("Confirm Undo"),
                                )
                                .clicked();
                            if confirm {
                                commands.push(BatchRenameUiCommand::UndoConfirmed(
                                    transaction_id.clone(),
                                ));
                                self.undo_confirmation = None;
                            }
                            if ui.button("Cancel").clicked() {
                                self.undo_confirmation = None;
                            }
                        });
                    });
                if !confirmation_open {
                    self.undo_confirmation = None;
                }
            } else {
                self.undo_confirmation = None;
            }
        }
    }

    fn cancel_apply_confirmation(&mut self) {
        self.apply_confirmation_open = false;
        self.pending_intent = None;
    }

    fn draw_header(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut Vec<BatchRenameUiCommand>,
        process_interactions: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        visual_order: &mut u64,
    ) {
        ui.collapsing("Batch rename safety and recovery", |ui| {
            ui.label(
                "Choose the root explicitly. Opening this dialog does not scan; direct children are included by default, and reparse points are excluded.",
            );
            ui.label(
                "Rules run in the displayed order. Numbering uses deterministic path order even when rows are filtered or sorted. Extensions are protected by default.",
            );
            ui.label(
                "Preview does not write and can become stale. Confirmation freezes the exact mapping; Apply rechecks source identities, parent folders, and destination names. Changed sources and newly occupied names are refused, and existing files are never overwritten. Same-folder staging supports case-only changes and rename cycles.",
            );
            ui.label(
                "Cancellation is checked between file moves. A failure or cancellation can leave a partial result; the receipt reports each file and keeps recovery paths when safe compensation is not possible. This is not an all-or-nothing rollback guarantee.",
            );
            ui.label(
                "Undo is available only while the recorded files still match their saved identities and bytes and each original name is free. Receipts persist in app data across restarts, with up to 32 retained; completed Undone receipts may be discarded to make room. Verification is limited to 1 GiB per file and 4 GiB per batch.",
            );
        });
        ui.horizontal_wrapped(|ui| {
            let choose_root = ui.button("Choose root folder…");
            register_batch_target(
                scope,
                ui,
                &choose_root,
                "choose-root",
                "Choose root folder",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if process_interactions && choose_root.clicked() {
                let mut picker = rfd::FileDialog::new();
                if let Some(root) = self.root_path.as_deref() {
                    picker = picker.set_directory(root);
                }
                if let Some(root) = picker.pick_folder() {
                    self.prepare_explicit_root_scan(root.clone());
                    commands.push(BatchRenameUiCommand::SelectRoot(root));
                }
            }
            let mut recursive = self.recursive;
            let recursive_response = ui.checkbox(&mut recursive, "Include subfolders");
            register_batch_target(
                scope,
                ui,
                &recursive_response,
                "recursive",
                "Include subfolders",
                crate::gui::jump_mode::TargetKind::Checkbox,
                visual_order,
            );
            let recursive_changed = recursive_response.changed();
            if process_interactions && recursive_changed {
                self.recursive = recursive;
                if self.root_path.is_some() {
                    self.invalidate_for_new_scan();
                    commands.push(BatchRenameUiCommand::RecursiveChanged);
                }
            }
            let rescan = ui.add_enabled(
                self.root_path.is_some() && !self.has_pending_scan(),
                egui::Button::new("Rescan"),
            );
            register_batch_target(
                scope,
                ui,
                &rescan,
                "rescan",
                "Rescan files",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if process_interactions && rescan.clicked() {
                commands.push(BatchRenameUiCommand::Rescan);
            }
        });

        let root = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.canonical_root())
            .or(self.root_path.as_deref());
        ui.horizontal(|ui| {
            ui.strong("Root:");
            if let Some(root) = root {
                let full = root.to_string_lossy().into_owned();
                ui.label(truncate_middle(&full, 112)).on_hover_text(full);
            } else {
                ui.label("No directory selected");
            }
            ui.label(if self.recursive {
                "Scope: recursive"
            } else {
                "Scope: direct children"
            });
        });
        match self.scan_banner {
            ScanBanner::ChooseRoot => {
                ui.label(
                    "Choose an explicit directory or rescan the retained root. The launcher never guesses a folder.",
                );
            }
            ScanBanner::Scanning => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        "Scanning this root. A preview is available only after a complete scan.",
                    );
                });
            }
            ScanBanner::Complete => {
                ui.colored_label(
                    ui.visuals().hyperlink_color,
                    "Complete scan. Preview is read-only.",
                );
            }
            ScanBanner::Incomplete(status) => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "Scan is {}. The partial results cannot be previewed.",
                        scan_status_label(status)
                    ),
                );
            }
            ScanBanner::Cancelled => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Scan cancelled. Choose Rescan to try again.",
                );
            }
            ScanBanner::Error => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    self.status_message
                        .as_deref()
                        .unwrap_or("The scan could not start."),
                );
            }
        }
        if let Some(message) = &self.status_message
            && self.scan_banner != ScanBanner::Error
        {
            ui.colored_label(ui.visuals().warn_fg_color, message);
        }

        if let Some(result) = &self.scan_result {
            let summary = result.summary();
            ui.label(format!(
                "{} file(s), {} folder(s), {} other entries; {} reparse point(s) excluded; {} issue(s).",
                summary.files_seen(),
                summary.directories_seen(),
                summary.other_entries_seen(),
                summary.reparse_entries_excluded(),
                summary.issue_count()
            ));
            if !summary.issues().is_empty() {
                egui::CollapsingHeader::new("Scan details")
                    .default_open(false)
                    .show(ui, |ui| {
                        for issue in summary.issues() {
                            let path = issue
                                .path()
                                .map(|path| format!("{}: ", path.display()))
                                .unwrap_or_default();
                            ui.label(format!("{path}{}", issue.message()));
                        }
                        if summary.issues_truncated() {
                            ui.label("Additional scan issues were omitted from this list.");
                        }
                    });
            }
        }
    }

    fn draw_rule_editor(
        &mut self,
        ui: &mut egui::Ui,
        process_interactions: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        visual_order: &mut u64,
    ) {
        ui.heading("Ordered filename rules");
        ui.label("Numbering follows the deterministic original path order, regardless of filtering or table sorting.");
        let mut rules = self.rules.clone();
        let mut extension_policy = self.extension_policy;
        let mut new_rule_kind = self.new_rule_kind;
        let mut rules_changed = false;
        ui.horizontal_wrapped(|ui| {
            let before = extension_policy;
            let extension_policy_identity = "extension-policy";
            let extension_response = egui::ComboBox::from_id_salt("batch_rename_extension_policy")
                .selected_text(match extension_policy {
                    ExtensionPolicy::Preserve => "Protect extension (default)",
                    ExtensionPolicy::Edit => "Edit full filename",
                })
                .show_ui(ui, |ui| {
                    let preserve = ui.selectable_value(
                        &mut extension_policy,
                        ExtensionPolicy::Preserve,
                        "Protect extension (default)",
                    );
                    register_batch_target(
                        scope
                            .map(|scope| scope.combo_popup_scope(extension_policy_identity))
                            .as_ref(),
                        ui,
                        &preserve,
                        "option/preserve",
                        "Protect extension",
                        crate::gui::jump_mode::TargetKind::Selectable,
                        visual_order,
                    );
                    let edit = ui.selectable_value(
                        &mut extension_policy,
                        ExtensionPolicy::Edit,
                        "Edit full filename",
                    );
                    register_batch_target(
                        scope
                            .map(|scope| scope.combo_popup_scope(extension_policy_identity))
                            .as_ref(),
                        ui,
                        &edit,
                        "option/edit",
                        "Edit full filename",
                        crate::gui::jump_mode::TargetKind::Selectable,
                        visual_order,
                    );
                });
            register_batch_target(
                scope,
                ui,
                &extension_response.response,
                extension_policy_identity,
                "Extension policy",
                crate::gui::jump_mode::TargetKind::ComboBox,
                visual_order,
            );
            rules_changed |= before != extension_policy;

            let new_rule_kind_identity = "new-rule-kind";
            let new_rule_response = egui::ComboBox::from_id_salt("batch_rename_new_rule")
                .selected_text(new_rule_label(new_rule_kind))
                .show_ui(ui, |ui| {
                    for kind in [
                        NewRuleKind::Prefix,
                        NewRuleKind::Suffix,
                        NewRuleKind::Number,
                        NewRuleKind::LiteralReplace,
                        NewRuleKind::RegexReplace,
                        NewRuleKind::Lowercase,
                        NewRuleKind::Uppercase,
                    ] {
                        let option =
                            ui.selectable_value(&mut new_rule_kind, kind, new_rule_label(kind));
                        register_batch_target(
                            scope
                                .map(|scope| scope.combo_popup_scope(new_rule_kind_identity))
                                .as_ref(),
                            ui,
                            &option,
                            new_rule_option_identity(kind),
                            new_rule_label(kind),
                            crate::gui::jump_mode::TargetKind::Selectable,
                            visual_order,
                        );
                    }
                });
            register_batch_target(
                scope,
                ui,
                &new_rule_response.response,
                new_rule_kind_identity,
                "New rule type",
                crate::gui::jump_mode::TargetKind::ComboBox,
                visual_order,
            );
            let add_rule = ui.button("Add rule");
            register_batch_target(
                scope,
                ui,
                &add_rule,
                "add-rule",
                "Add rule",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if add_rule.clicked() && process_interactions {
                let id = self.next_rule_id;
                self.next_rule_id = self.next_rule_id.wrapping_add(1);
                rules.push(RuleDraft {
                    id,
                    rule: default_rule(new_rule_kind),
                });
                rules_changed = true;
            }
        });

        let mut rule_action = None;
        let rule_count = rules.len();
        for index in 0..rule_count {
            let action = {
                let draft = &mut rules[index];
                let rule_ui_id = egui::Id::new(("batch_rename_rule", self.session_epoch, draft.id));
                ui.scope_builder(egui::UiBuilder::new().id(rule_ui_id), |ui| {
                    draw_rule_row(
                        ui,
                        draft.id,
                        &mut draft.rule,
                        index,
                        rule_count,
                        scope,
                        visual_order,
                    )
                })
                .inner
            };
            let (changed, action) = action;
            rules_changed |= process_interactions && changed;
            if process_interactions && action != RuleRowAction::None && rule_action.is_none() {
                rule_action = Some((index, action));
            }
        }

        if let Some((index, action)) = rule_action {
            match action {
                RuleRowAction::None => {}
                RuleRowAction::MoveUp if index > 0 => {
                    rules.swap(index, index - 1);
                    rules_changed = true;
                }
                RuleRowAction::MoveDown if index + 1 < rules.len() => {
                    rules.swap(index, index + 1);
                    rules_changed = true;
                }
                RuleRowAction::Remove => {
                    rules.remove(index);
                    rules_changed = true;
                }
                RuleRowAction::MoveUp | RuleRowAction::MoveDown => {}
            }
        }

        if process_interactions {
            self.new_rule_kind = new_rule_kind;
        }
        if rules_changed && process_interactions {
            self.rules = rules;
            self.extension_policy = extension_policy;
            self.rules_revision = self.rules_revision.wrapping_add(1);
            self.rebuild_preview();
        }
        if let Some(preview) = &self.verified_preview {
            for error in preview.preview().rule_errors() {
                let number = error
                    .rule_index
                    .map(|index| format!("Rule {}: ", index + 1))
                    .unwrap_or_default();
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{number}{:?}", error.kind),
                );
            }
        }
    }

    fn draw_preview(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut Vec<BatchRenameUiCommand>,
        process_interactions: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        visual_order: &mut u64,
    ) {
        self.draw_folder_tree(ui, commands, process_interactions);
        if let Some(root) = commands.iter().find_map(|command| match command {
            BatchRenameUiCommand::SelectRoot(root) => Some(root.clone()),
            _ => None,
        }) && process_interactions
        {
            self.prepare_explicit_root_scan(root);
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Filter files:");
            let mut filter = self.filter.clone();
            let response = ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .desired_width(240.0)
                    .hint_text("Name, path, or extension"),
            );
            register_batch_target(
                scope,
                ui,
                &response,
                "filter",
                "Filter preview files",
                crate::gui::jump_mode::TargetKind::TextEntry,
                visual_order,
            );
            #[cfg(test)]
            {
                self.filter_widget_id = Some(response.id);
            }
            if process_interactions && response.changed() {
                self.filter = filter;
                self.rebuild_visible_rows();
            }
            let select_visible = ui.add_enabled(
                !self.visible_rows.is_empty(),
                egui::Button::new("Select visible"),
            );
            register_batch_target(
                scope,
                ui,
                &select_visible,
                "select-visible",
                "Select visible files",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if process_interactions && select_visible.clicked() {
                self.select_visible(true);
            }
            let clear_visible = ui.add_enabled(
                !self.visible_rows.is_empty(),
                egui::Button::new("Clear visible"),
            );
            register_batch_target(
                scope,
                ui,
                &clear_visible,
                "clear-visible",
                "Clear visible selection",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if process_interactions && clear_visible.clicked() {
                self.select_visible(false);
            }
        });

        self.draw_preview_summary(ui);
        if self.snapshot.is_none() {
            ui.label("A complete scan is required before files can be selected or previewed.");
            return;
        }
        if self.display_rows.is_empty() {
            ui.label("No regular files were found in this scope.");
            return;
        }
        self.draw_file_table(ui, process_interactions);
    }

    fn draw_folder_tree(
        &self,
        ui: &mut egui::Ui,
        commands: &mut Vec<BatchRenameUiCommand>,
        process_interactions: bool,
    ) {
        egui::CollapsingHeader::new("Folders in this scan")
            .default_open(false)
            .show(ui, |ui| {
                if self.folder_tree.is_empty() {
                    ui.label("No subfolders were found.");
                }
                if let Some(generation) = self.scan_generation {
                    draw_folder_nodes(
                        ui,
                        &self.folder_tree,
                        generation,
                        commands,
                        process_interactions,
                    );
                }
            });
    }

    fn draw_preview_summary(&self, ui: &mut egui::Ui) {
        let mut selected = 0usize;
        let mut actionable = 0usize;
        let mut no_op = 0usize;
        let mut conflicted = 0usize;
        let mut excluded = 0usize;
        for row in &self.display_rows {
            match &row.status {
                PreviewRowStatus::Ready => {
                    selected += 1;
                    actionable += 1;
                }
                PreviewRowStatus::NoOp => {
                    selected += 1;
                    no_op += 1;
                }
                PreviewRowStatus::Invalid(_) => selected += 1,
                PreviewRowStatus::Conflict(_) => {
                    selected += 1;
                    conflicted += 1;
                }
                PreviewRowStatus::Excluded => excluded += 1,
                PreviewRowStatus::Unselected | PreviewRowStatus::Ineligible => {}
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Selected: {selected}"));
            ui.label(format!("Actionable: {actionable}"));
            ui.label(format!("No-op: {no_op}"));
            ui.label(format!("Conflicted: {conflicted}"));
            ui.label(format!("Excluded: {excluded}"));
        });
        if let Some(preview) = &self.verified_preview {
            let message = match preview.preview().readiness() {
                PreviewReadiness::EmptySelection => "Select files to create a preview.",
                PreviewReadiness::AllNoOp => "The selected names are unchanged.",
                PreviewReadiness::Ready => "The selected preview has no naming conflicts.",
                PreviewReadiness::Invalid => "Resolve every invalid name or conflict.",
            };
            let color = if preview.preview().readiness() == PreviewReadiness::Invalid {
                ui.visuals().warn_fg_color
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(color, message);
        }
    }

    fn draw_file_table(&mut self, ui: &mut egui::Ui, process_interactions: bool) {
        let mut requested_sort = None;
        let mut selection_updates = Vec::new();
        let mut inclusion_updates = Vec::new();
        let generation = self.scan_generation.unwrap_or(RenameScanGeneration(0));
        let sort = self.visual_sort;
        let display_rows = &self.display_rows;
        let visible_rows = &self.visible_rows;
        let selected_by_ref = &self.selected_by_ref;
        let included_by_ref = &self.included_by_ref;

        #[cfg(test)]
        let mut status_column_width = 0.0;
        #[cfg(test)]
        let mut rendered_rows = 0;
        let _table_output = egui::ScrollArea::horizontal()
            .id_salt("batch_rename_table_horizontal_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(1120.0);
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(58.0).clip(true))
                    .column(Column::exact(58.0).clip(true))
                    .column(Column::initial(170.0).clip(true))
                    .column(Column::initial(180.0).clip(true))
                    .column(Column::initial(300.0).clip(true))
                    .column(Column::initial(210.0).clip(true))
                    .column(Column::remainder().at_least(180.0).clip(true));
                table = table.sense(egui::Sense::click());
                table
                    .header(PREVIEW_ROW_HEIGHT, |mut header| {
                        header.col(|ui| {
                            ui.strong("Include");
                        });
                        header.col(|ui| {
                            ui.strong("Select");
                        });
                        header.col(|ui| {
                            if sort_button(
                                ui,
                                "Original filename",
                                sort,
                                VisualSortColumn::OriginalName,
                            ) {
                                requested_sort = Some(VisualSortColumn::OriginalName);
                            }
                        });
                        header.col(|ui| {
                            if sort_button(
                                ui,
                                "Relative path",
                                sort,
                                VisualSortColumn::RelativePath,
                            ) {
                                requested_sort = Some(VisualSortColumn::RelativePath);
                            }
                        });
                        header.col(|ui| {
                            if sort_button(
                                ui,
                                "Original full path",
                                sort,
                                VisualSortColumn::FullPath,
                            ) {
                                requested_sort = Some(VisualSortColumn::FullPath);
                            }
                        });
                        header.col(|ui| {
                            if sort_button(
                                ui,
                                "Proposed path",
                                sort,
                                VisualSortColumn::ProposedPath,
                            ) {
                                requested_sort = Some(VisualSortColumn::ProposedPath);
                            }
                        });
                        header.col(|ui| {
                            #[cfg(test)]
                            {
                                status_column_width = ui.available_width();
                            }
                            if sort_button(ui, "Status", sort, VisualSortColumn::Status) {
                                requested_sort = Some(VisualSortColumn::Status);
                            }
                        });
                    })
                    .body(|body| {
                        body.rows(PREVIEW_ROW_HEIGHT, visible_rows.len(), |mut row| {
                            #[cfg(test)]
                            {
                                rendered_rows += 1;
                            }
                            let Some(display_index) = visible_rows.get(row.index()).copied() else {
                                return;
                            };
                            let Some(display) = display_rows.get(display_index) else {
                                return;
                            };
                            let entry_ref = display.entry_ref;
                            let row_id =
                                egui::Id::new(("batch_rename_entry", generation.0, entry_ref.0));
                            let included = included_by_ref.get(&entry_ref).copied().unwrap_or(true);
                            let selected =
                                selected_by_ref.get(&entry_ref).copied().unwrap_or(false);
                            let mut included_value = included;
                            let mut selected_value = selected;
                            row.col(|ui| {
                                ui.push_id((row_id, "include"), |ui| {
                                    if ui
                                        .checkbox(&mut included_value, "")
                                        .on_hover_text("Include this file in preview analysis")
                                        .changed()
                                    {
                                        inclusion_updates.push((entry_ref, included_value));
                                    }
                                });
                            });
                            row.col(|ui| {
                                ui.push_id((row_id, "select"), |ui| {
                                    if ui
                                        .checkbox(&mut selected_value, "")
                                        .on_hover_text("Select this file for the preview")
                                        .changed()
                                    {
                                        selection_updates.push((entry_ref, selected_value));
                                    }
                                });
                            });
                            row.col(|ui| {
                                let text = display.source_name.to_string_lossy().into_owned();
                                ui.label(truncate_middle(&text, PATH_PREVIEW_CHARS))
                                    .on_hover_text(text);
                            });
                            row.col(|ui| {
                                let text = display.relative_path.to_string_lossy().into_owned();
                                ui.label(truncate_middle(&text, PATH_PREVIEW_CHARS))
                                    .on_hover_text(text);
                            });
                            row.col(|ui| {
                                let text = display.source_path.to_string_lossy().into_owned();
                                ui.label(truncate_middle(&text, PATH_PREVIEW_CHARS))
                                    .on_hover_text(text);
                            });
                            row.col(|ui| {
                                let text = display
                                    .proposed_path
                                    .as_deref()
                                    .map(|path| path.to_string_lossy().into_owned())
                                    .or_else(|| {
                                        display
                                            .proposed_name
                                            .as_ref()
                                            .map(|name| name.to_string_lossy().into_owned())
                                    })
                                    .unwrap_or_else(|| "—".to_owned());
                                ui.label(truncate_middle(&text, PATH_PREVIEW_CHARS))
                                    .on_hover_text(text);
                            });
                            row.col(|ui| {
                                let text = row_status_description(
                                    &display.status,
                                    self.scan_result.as_ref(),
                                );
                                let label = truncate_middle(&text, 56);
                                let color = match &display.status {
                                    PreviewRowStatus::Ready => ui.visuals().hyperlink_color,
                                    PreviewRowStatus::Conflict(_)
                                    | PreviewRowStatus::Invalid(_) => ui.visuals().error_fg_color,
                                    PreviewRowStatus::NoOp => ui.visuals().weak_text_color(),
                                    _ => ui.visuals().text_color(),
                                };
                                ui.colored_label(color, label).on_hover_text(text);
                            });
                        });
                    });
            });

        #[cfg(test)]
        {
            self.last_table_scroll_metrics = Some((
                _table_output.id,
                _table_output.inner_rect.width(),
                _table_output.content_size.x,
                status_column_width,
                _table_output.state.offset.x,
            ));
            self.last_table_rendered_rows = rendered_rows;
        }

        if process_interactions && let Some(column) = requested_sort {
            if self.visual_sort.column == column {
                self.visual_sort.descending = !self.visual_sort.descending;
            } else {
                self.visual_sort = VisualSort {
                    column,
                    descending: false,
                };
            }
            self.rebuild_visible_rows();
        }

        if process_interactions && (!selection_updates.is_empty() || !inclusion_updates.is_empty())
        {
            let mut updates: HashMap<SnapshotEntryRef, (Option<bool>, Option<bool>)> =
                HashMap::new();
            for (entry_ref, selected) in selection_updates {
                updates.entry(entry_ref).or_default().0 = Some(selected);
            }
            for (entry_ref, included) in inclusion_updates {
                updates.entry(entry_ref).or_default().1 = Some(included);
            }
            let updates: Vec<_> = updates
                .into_iter()
                .map(|(entry_ref, (selected, included))| (entry_ref, selected, included))
                .collect();
            self.apply_row_updates(&updates);
        }
    }
}

fn draw_rule_row(
    ui: &mut egui::Ui,
    id: u64,
    rule: &mut RenameRule,
    index: usize,
    total: usize,
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    visual_order: &mut u64,
) -> (bool, RuleRowAction) {
    let mut changed = false;
    let mut action = RuleRowAction::None;
    let rule_identity = format!("rule/{id}");
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{}.", index + 1));
        let move_up = ui
            .add_enabled(index > 0, egui::Button::new("↑").small())
            .on_hover_text("Move rule earlier");
        register_batch_target(
            scope,
            ui,
            &move_up,
            format!("{rule_identity}/move-up"),
            "Move rule earlier",
            crate::gui::jump_mode::TargetKind::Button,
            visual_order,
        );
        if move_up.clicked() {
            action = RuleRowAction::MoveUp;
        }
        let move_down = ui
            .add_enabled(index + 1 < total, egui::Button::new("↓").small())
            .on_hover_text("Move rule later");
        register_batch_target(
            scope,
            ui,
            &move_down,
            format!("{rule_identity}/move-down"),
            "Move rule later",
            crate::gui::jump_mode::TargetKind::Button,
            visual_order,
        );
        if move_down.clicked() {
            action = RuleRowAction::MoveDown;
        }
        let remove = ui.button("×").on_hover_text("Remove rule");
        register_batch_target(
            scope,
            ui,
            &remove,
            format!("{rule_identity}/remove"),
            "Remove rule",
            crate::gui::jump_mode::TargetKind::Button,
            visual_order,
        );
        if remove.clicked() {
            action = RuleRowAction::Remove;
        }
        match rule {
            RenameRule::Prefix(value) => {
                ui.label("Prefix");
                let response = ui.add(egui::TextEdit::singleline(value).desired_width(180.0));
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/prefix"),
                    "Prefix text",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    visual_order,
                );
                changed |= response.changed();
            }
            RenameRule::Suffix(value) => {
                ui.label("Suffix");
                let response = ui.add(egui::TextEdit::singleline(value).desired_width(180.0));
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/suffix"),
                    "Suffix text",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    visual_order,
                );
                changed |= response.changed();
            }
            RenameRule::Number {
                start,
                step,
                padding,
            } => {
                ui.label("Number");
                ui.label("Start");
                let response = ui.add(egui::DragValue::new(start));
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/number-start"),
                    "Number start",
                    crate::gui::jump_mode::TargetKind::Slider,
                    visual_order,
                );
                changed |= response.changed();
                ui.label("Step");
                let response = ui.add(egui::DragValue::new(step));
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/number-step"),
                    "Number step",
                    crate::gui::jump_mode::TargetKind::Slider,
                    visual_order,
                );
                changed |= response.changed();
                ui.label("Padding");
                let response = ui.add(egui::DragValue::new(padding).range(0..=32));
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/number-padding"),
                    "Number padding",
                    crate::gui::jump_mode::TargetKind::Slider,
                    visual_order,
                );
                changed |= response.changed();
            }
            RenameRule::LiteralReplace { find, replace_with } => {
                ui.label("Literal replace");
                let response = ui.add(
                    egui::TextEdit::singleline(find)
                        .desired_width(130.0)
                        .hint_text("Find"),
                );
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/literal-find"),
                    "Find text",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    visual_order,
                );
                changed |= response.changed();
                ui.label("with");
                let response = ui.add(
                    egui::TextEdit::singleline(replace_with)
                        .desired_width(130.0)
                        .hint_text("Replace with"),
                );
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/literal-replacement"),
                    "Replacement text",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    visual_order,
                );
                changed |= response.changed();
            }
            RenameRule::RegexReplace {
                pattern,
                replace_with,
            } => {
                ui.label("Regex replace");
                let response = ui.add(
                    egui::TextEdit::singleline(pattern)
                        .desired_width(130.0)
                        .hint_text("Pattern"),
                );
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/regex-pattern"),
                    "Regex pattern",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    visual_order,
                );
                changed |= response.changed();
                ui.label("with");
                let response = ui.add(
                    egui::TextEdit::singleline(replace_with)
                        .desired_width(130.0)
                        .hint_text("Replacement, e.g. $1"),
                );
                register_batch_target(
                    scope,
                    ui,
                    &response,
                    format!("{rule_identity}/regex-replacement"),
                    "Regex replacement",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    visual_order,
                );
                changed |= response.changed();
            }
            RenameRule::Case(case) => {
                ui.label("Case");
                let before = *case;
                let case_identity = format!("{rule_identity}/case");
                let popup_scope = scope.map(|scope| scope.combo_popup_scope(&case_identity));
                let combo = egui::ComboBox::from_id_salt(("batch_rename_case", id))
                    .selected_text(match case {
                        CaseConversion::Lowercase => "Lowercase",
                        CaseConversion::Uppercase => "Uppercase",
                    })
                    .show_ui(ui, |ui| {
                        let lowercase =
                            ui.selectable_value(case, CaseConversion::Lowercase, "Lowercase");
                        register_batch_target(
                            popup_scope.as_ref(),
                            ui,
                            &lowercase,
                            "option/lowercase",
                            "Lowercase",
                            crate::gui::jump_mode::TargetKind::Selectable,
                            visual_order,
                        );
                        let uppercase =
                            ui.selectable_value(case, CaseConversion::Uppercase, "Uppercase");
                        register_batch_target(
                            popup_scope.as_ref(),
                            ui,
                            &uppercase,
                            "option/uppercase",
                            "Uppercase",
                            crate::gui::jump_mode::TargetKind::Selectable,
                            visual_order,
                        );
                    });
                register_batch_target(
                    scope,
                    ui,
                    &combo.response,
                    &case_identity,
                    "Case conversion",
                    crate::gui::jump_mode::TargetKind::ComboBox,
                    visual_order,
                );
                changed |= before != *case;
            }
        }
    });
    (changed, action)
}

fn register_batch_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) {
    let _ = crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        local_identity,
        label,
        kind,
        visual_order,
    );
}

fn new_rule_option_identity(kind: NewRuleKind) -> &'static str {
    match kind {
        NewRuleKind::Prefix => "option/prefix",
        NewRuleKind::Suffix => "option/suffix",
        NewRuleKind::Number => "option/number",
        NewRuleKind::LiteralReplace => "option/literal-replace",
        NewRuleKind::RegexReplace => "option/regex-replace",
        NewRuleKind::Lowercase => "option/lowercase",
        NewRuleKind::Uppercase => "option/uppercase",
    }
}

fn default_rule(kind: NewRuleKind) -> RenameRule {
    match kind {
        NewRuleKind::Prefix => RenameRule::Prefix(String::new()),
        NewRuleKind::Suffix => RenameRule::Suffix(String::new()),
        NewRuleKind::Number => RenameRule::Number {
            start: 1,
            step: 1,
            padding: 2,
        },
        NewRuleKind::LiteralReplace => RenameRule::LiteralReplace {
            find: String::new(),
            replace_with: String::new(),
        },
        NewRuleKind::RegexReplace => RenameRule::RegexReplace {
            pattern: String::new(),
            replace_with: String::new(),
        },
        NewRuleKind::Lowercase => RenameRule::Case(CaseConversion::Lowercase),
        NewRuleKind::Uppercase => RenameRule::Case(CaseConversion::Uppercase),
    }
}

fn new_rule_label(kind: NewRuleKind) -> &'static str {
    match kind {
        NewRuleKind::Prefix => "Prefix",
        NewRuleKind::Suffix => "Suffix",
        NewRuleKind::Number => "Number sequence",
        NewRuleKind::LiteralReplace => "Literal replace",
        NewRuleKind::RegexReplace => "Regex replace",
        NewRuleKind::Lowercase => "Lowercase",
        NewRuleKind::Uppercase => "Uppercase",
    }
}

fn sort_button(ui: &mut egui::Ui, label: &str, sort: VisualSort, column: VisualSortColumn) -> bool {
    let indicator = if sort.column == column {
        if sort.descending { " ↓" } else { " ↑" }
    } else {
        ""
    };
    ui.button(format!("{label}{indicator}")).clicked()
}

fn status_sort_key(status: &PreviewRowStatus) -> u8 {
    match status {
        PreviewRowStatus::Ready => 0,
        PreviewRowStatus::NoOp => 1,
        PreviewRowStatus::Invalid(_) => 2,
        PreviewRowStatus::Conflict(_) => 3,
        PreviewRowStatus::Unselected => 4,
        PreviewRowStatus::Excluded => 5,
        PreviewRowStatus::Ineligible => 6,
    }
}

fn row_status_description(status: &PreviewRowStatus, scan: Option<&RenameScanResult>) -> String {
    match status {
        PreviewRowStatus::Ready => "Ready".to_owned(),
        PreviewRowStatus::NoOp => "No-op: the proposed name is unchanged".to_owned(),
        PreviewRowStatus::Invalid(reason) => format!("Invalid: {reason:?}"),
        PreviewRowStatus::Conflict(batch_rename::ConflictReason::DuplicateSourcePath) => {
            "Conflict: duplicate source path".to_owned()
        }
        PreviewRowStatus::Conflict(batch_rename::ConflictReason::DuplicateProposedTarget) => {
            "Conflict: another selected file proposes the same target".to_owned()
        }
        PreviewRowStatus::Conflict(batch_rename::ConflictReason::DestinationOccupied {
            occupant,
        }) => {
            let path = scan
                .into_iter()
                .flat_map(|result| result.rows())
                .find(|row| row.entry_ref() == *occupant)
                .map(|row| row.path().display().to_string())
                .unwrap_or_else(|| "another entry in this folder".to_owned());
            format!("Conflict: target is already occupied by {path}")
        }
        PreviewRowStatus::Unselected => "Not selected".to_owned(),
        PreviewRowStatus::Excluded => "Excluded from preview".to_owned(),
        PreviewRowStatus::Ineligible => "This entry cannot be renamed".to_owned(),
    }
}

fn scan_status_label(status: RenameScanStatus) -> &'static str {
    match status {
        RenameScanStatus::Complete => "complete",
        RenameScanStatus::Partial => "partial",
        RenameScanStatus::InvalidRoot => "invalid root",
        RenameScanStatus::Inaccessible => "inaccessible",
        RenameScanStatus::Unsupported => "unsupported",
        RenameScanStatus::Cancelled => "cancelled",
        RenameScanStatus::ResourceLimit => "resource limit reached",
        RenameScanStatus::ChangedDuringScan => "changed while scanning",
    }
}

fn truncate_middle(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars || max_chars < 5 {
        return text.to_owned();
    }
    let left_count = (max_chars - 1) / 2;
    let right_count = max_chars - left_count - 1;
    let left: String = text.chars().take(left_count).collect();
    let right: String = text
        .chars()
        .rev()
        .take(right_count)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{left}…{right}")
}

fn build_folder_tree(result: &RenameScanResult) -> Vec<FolderNode> {
    let Some(root) = result.canonical_root() else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    for row in result
        .rows()
        .iter()
        .filter(|row| row.kind() == SnapshotEntryKind::Directory && !row.is_reparse_point())
    {
        let mut parent = &mut roots;
        let mut full = root.to_path_buf();
        for component in row.relative_path().components() {
            let Component::Normal(name) = component else {
                continue;
            };
            full.push(name);
            let index = match parent
                .iter()
                .position(|node: &FolderNode| node.path == full)
            {
                Some(index) => index,
                None => {
                    parent.push(FolderNode {
                        path: full.clone(),
                        name: name.to_os_string(),
                        children: Vec::new(),
                    });
                    parent.len() - 1
                }
            };
            parent = &mut parent[index].children;
        }
    }
    sort_folder_nodes(&mut roots);
    roots
}

fn sort_folder_nodes(nodes: &mut [FolderNode]) {
    nodes.sort_by(|left, right| {
        batch_rename::compare_paths_windows_ordinal(&left.path, &right.path)
            .then_with(|| left.path.cmp(&right.path))
    });
    for node in nodes {
        sort_folder_nodes(&mut node.children);
    }
}

fn draw_folder_nodes(
    ui: &mut egui::Ui,
    nodes: &[FolderNode],
    generation: RenameScanGeneration,
    commands: &mut Vec<BatchRenameUiCommand>,
    process_interactions: bool,
) {
    for node in nodes {
        let id = egui::Id::new(("batch_rename_folder", generation.0, &node.path));
        ui.push_id(id, |ui| {
            ui.horizontal(|ui| {
                let label = node.name.to_string_lossy();
                let choose_root = ui.small_button("Use as root").clicked();
                if process_interactions && choose_root {
                    commands.push(BatchRenameUiCommand::SelectRoot(node.path.clone()));
                }
                egui::CollapsingHeader::new(label.as_ref())
                    .id_salt(id)
                    .show(ui, |ui| {
                        draw_folder_nodes(
                            ui,
                            &node.children,
                            generation,
                            commands,
                            process_interactions,
                        )
                    });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;
    use std::time::{Duration, Instant};

    fn fixture_root() -> PathBuf {
        let parent = PathBuf::from("target/productivity_expansion");
        fs::create_dir_all(&parent).unwrap();
        let directory = tempfile::Builder::new()
            .prefix("batch_rename_ui_")
            .tempdir_in(parent)
            .unwrap();
        let root = directory.keep();
        fs::write(root.join("alpha.txt"), "alpha").unwrap();
        fs::write(root.join("beta.txt"), "beta").unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        fs::write(root.join("nested/inside.txt"), "inside").unwrap();
        root
    }

    fn finish_scan(state: &mut BatchRenameDialogState) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.has_pending_scan() && Instant::now() < deadline {
            state.poll_scan();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!state.has_pending_scan(), "scan fixture should complete");
        assert_eq!(state.scan_banner, ScanBanner::Complete);
    }

    fn run_dialog_frame(
        state: &mut BatchRenameDialogState,
        context: &egui::Context,
        input: egui::RawInput,
    ) -> Vec<BatchRenameUiCommand> {
        run_dialog_frame_with_runtime(state, context, input, &RenameRuntimeSnapshot::default())
    }

    fn run_dialog_frame_with_runtime(
        state: &mut BatchRenameDialogState,
        context: &egui::Context,
        input: egui::RawInput,
        runtime: &RenameRuntimeSnapshot,
    ) -> Vec<BatchRenameUiCommand> {
        let mut commands = Vec::new();
        let output = context.run_ui(input, |ctx| {
            let first_pass = ctx.current_pass_index() == 0;
            commands = state.ui(ctx, first_pass, runtime);
        });
        output.drop_without_applying_deltas();
        commands
    }

    fn screen_input(size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        }
    }

    #[test]
    fn jump_mode_batch_rename_form_withholds_parent_scope_for_transient_owners() {
        let mut state = BatchRenameDialogState::default();
        state.open();
        let initial_generation = state.jump_mode_target_generation();
        assert!(state.jump_mode_parent_targets_allowed());

        state.apply_confirmation_open = true;
        assert!(!state.jump_mode_parent_targets_allowed());
        state.apply_confirmation_open = false;
        state.undo_confirmation = Some("fixture-transaction".to_owned());
        assert!(!state.jump_mode_parent_targets_allowed());
        state.undo_confirmation = None;
        state.operation_was_active = true;
        assert!(!state.jump_mode_parent_targets_allowed());
        state.observe_runtime(&RenameRuntimeSnapshot::default());

        state.end_session();
        assert!(!state.jump_mode_parent_targets_allowed());
        state.open();
        assert!(state.jump_mode_parent_targets_allowed());
        assert!(state.jump_mode_target_generation() > initial_generation);
    }

    #[cfg(windows)]
    fn app_fixture_workspace() -> tempfile::TempDir {
        let parent = PathBuf::from("target/productivity_expansion");
        fs::create_dir_all(&parent).expect("create disposable M8-E parent");
        tempfile::Builder::new()
            .prefix("batch_rename_app_flow_")
            .tempdir_in(parent)
            .expect("create disposable M8-E app fixture")
    }

    #[cfg(windows)]
    struct NotesDirectoryOverride(Option<std::ffi::OsString>);

    #[cfg(windows)]
    impl NotesDirectoryOverride {
        fn use_fixture(path: &Path) -> Self {
            let previous = std::env::var_os("ML_NOTES_DIR");
            unsafe { std::env::set_var("ML_NOTES_DIR", path) };
            Self(previous)
        }
    }

    #[cfg(windows)]
    impl Drop for NotesDirectoryOverride {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                unsafe { std::env::set_var("ML_NOTES_DIR", previous) };
            } else {
                unsafe { std::env::remove_var("ML_NOTES_DIR") };
            }
        }
    }

    #[cfg(windows)]
    fn app_for_fixture(ctx: &egui::Context, fixture: &Path) -> crate::gui::LauncherApp {
        let app_data = fixture.join("app-data");
        fs::create_dir_all(&app_data).expect("create fixture app-data root");
        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.dashboard.enabled = false;
        settings.dashboard.config_path = Some(
            app_data
                .join("dashboard.json")
                .to_string_lossy()
                .into_owned(),
        );
        let mut app = crate::gui::LauncherApp::new(
            ctx,
            std::sync::Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test_at(&app_data),
            app_data.join("actions.json").to_string_lossy().into_owned(),
            app_data
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        app.watchers.clear();
        app.clipboard_modify_watcher = None;
        app
    }

    #[cfg(windows)]
    fn finish_app_scan(app: &mut crate::gui::LauncherApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.batch_rename_dialog.has_pending_scan() && Instant::now() < deadline {
            app.poll_batch_rename_runtime(ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        app.poll_batch_rename_runtime(ctx);
        assert!(
            !app.batch_rename_dialog.has_pending_scan(),
            "the app-owned rename scan should complete"
        );
        assert!(app.batch_rename_dialog.has_accepted_snapshot());
        assert!(app.batch_rename_runtime_snapshot().journal_ready);
    }

    #[cfg(windows)]
    fn finish_app_operation(
        app: &mut crate::gui::LauncherApp,
        ctx: &egui::Context,
    ) -> RenameRuntimeSnapshot {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            app.poll_batch_rename_runtime(ctx);
            let runtime = app.batch_rename_runtime_snapshot();
            if !runtime.operation_active && runtime.last_message.is_some() {
                return runtime;
            }
            assert!(
                Instant::now() < deadline,
                "the app-owned rename operation should finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn scan_identities(
        state: &BatchRenameDialogState,
    ) -> std::collections::HashMap<OsString, crate::file_operations::NativeFileIdentity> {
        state
            .scan_result
            .as_ref()
            .expect("complete scan result")
            .rows()
            .iter()
            .filter(|row| row.kind() == SnapshotEntryKind::File)
            .map(|row| {
                (
                    row.path()
                        .file_name()
                        .expect("file row has a name")
                        .to_os_string(),
                    row.identity(),
                )
            })
            .collect()
    }

    #[cfg(windows)]
    fn file_bytes(root: &Path) -> std::collections::BTreeMap<OsString, Vec<u8>> {
        fs::read_dir(root)
            .expect("read fixture root")
            .map(|entry| {
                let entry = entry.expect("read fixture entry");
                assert!(
                    entry
                        .file_type()
                        .expect("read fixture entry type")
                        .is_file()
                );
                (
                    entry.file_name(),
                    fs::read(entry.path()).expect("read fixture file bytes"),
                )
            })
            .collect()
    }

    #[test]
    fn m8c_discarded_pass_keeps_the_filter_draft_single_applied() {
        let context = egui::Context::default();
        let size = egui::vec2(1280.0, 900.0);
        let mut state = BatchRenameDialogState::default();
        state.open();
        run_dialog_frame(&mut state, &context, screen_input(size, Vec::new()));
        let filter_widget_id = state.filter_widget_id.unwrap();
        context.memory_mut(|memory| memory.request_focus(filter_widget_id));

        let mut pass_count = 0;
        let output = context.run_ui(
            screen_input(size, vec![egui::Event::Text("beta".to_owned())]),
            |ctx| {
                let first_pass = ctx.current_pass_index() == 0;
                let _ = state.ui(ctx, first_pass, &RenameRuntimeSnapshot::default());
                if first_pass {
                    ctx.request_discard("batch rename filter edit test");
                }
                pass_count += 1;
            },
        );
        output.drop_without_applying_deltas();
        assert!(pass_count > 1, "the fixture must exercise a discarded pass");
        assert_eq!(state.filter, "beta");
    }

    #[test]
    fn m8c_narrow_table_scroll_reaches_status_and_keeps_rows_virtualized() {
        let context = egui::Context::default();
        let size = egui::vec2(520.0, 360.0);
        let mut state = BatchRenameDialogState::default();
        state.display_rows = (0..500)
            .map(|index| {
                let file_name = format!("file-{index:04}.txt");
                let source_path = PathBuf::from(format!(r"C:\fixture\{file_name}"));
                PreviewDisplayRow {
                    entry_ref: SnapshotEntryRef(index),
                    relative_path: PathBuf::from(&file_name),
                    source_path: source_path.clone(),
                    source_name: OsString::from(file_name),
                    proposed_path: Some(source_path),
                    proposed_name: None,
                    status: PreviewRowStatus::Ready,
                }
            })
            .collect();
        state.visible_rows = (0..state.display_rows.len()).collect();
        state.scan_generation = Some(RenameScanGeneration(9));

        let input = screen_input(size, Vec::new());
        let output = context.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                state.draw_file_table(ui, false);
            });
        });
        output.drop_without_applying_deltas();
        let (scroll_id, viewport_width, content_width, status_width, _) =
            state.last_table_scroll_metrics.unwrap();
        assert!(content_width > viewport_width + 100.0);
        assert!(status_width >= 120.0);
        assert!(state.last_table_rendered_rows < state.visible_rows.len());

        let mut scroll = egui::containers::scroll_area::State::load(&context, scroll_id).unwrap();
        scroll.offset.x = content_width - viewport_width;
        scroll.store(&context, scroll_id);
        let output = context.run_ui(screen_input(size, Vec::new()), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                state.draw_file_table(ui, false);
            });
        });
        output.drop_without_applying_deltas();
        let (_, viewport_width, content_width, status_width, offset) =
            state.last_table_scroll_metrics.unwrap();
        assert!(status_width >= 120.0);
        assert!(offset > 0.0);
        assert!(offset + viewport_width >= content_width - 1.0);
        assert!(state.last_table_rendered_rows < state.visible_rows.len());
    }

    #[test]
    #[cfg(windows)]
    fn m8c_explicit_root_preview_is_cached_stable_and_read_only() {
        let ctx = egui::Context::default();
        let root = fixture_root();
        let expected_files = vec![
            ("alpha.txt".to_owned(), "alpha".to_owned()),
            ("beta.txt".to_owned(), "beta".to_owned()),
        ];
        let mut state = BatchRenameDialogState::default();
        state.open();
        assert!(state.root_path().is_none());
        assert!(
            !state.has_pending_scan(),
            "open must not infer or scan a root"
        );
        for _ in 0..3 {
            let commands = run_dialog_frame(
                &mut state,
                &ctx,
                screen_input(egui::vec2(1280.0, 900.0), Vec::new()),
            );
            assert!(commands.is_empty());
        }
        assert!(state.root_path().is_none());
        assert!(!state.has_pending_scan());

        let coordinator = RenameScanCoordinator::new().unwrap();
        state.start_scan(root.clone(), &coordinator);
        assert!(state.has_pending_scan());
        finish_scan(&mut state);
        assert_eq!(state.snapshot.as_ref().unwrap().canonical_root(), root);
        assert_eq!(
            state.scan_result.as_ref().unwrap().scope(),
            crate::file_operations::RenameScanScope::DirectChildren
        );
        assert_eq!(
            state.display_rows.len(),
            2,
            "directory rows are not rename candidates"
        );
        assert_eq!(
            state.folder_tree.len(),
            1,
            "subfolders are offered as bounded roots"
        );
        assert!(
            state
                .scan_result
                .as_ref()
                .unwrap()
                .rows()
                .iter()
                .all(|row| row.path().file_name() != Some(OsStr::new("inside.txt")))
        );
        let initial_selection_revision = state.snapshot.as_ref().unwrap().selection_revision().0;
        state.select_visible(true);
        assert_eq!(
            state.snapshot.as_ref().unwrap().selection_revision().0,
            initial_selection_revision + 1
        );
        state.select_visible(true);
        assert_eq!(
            state.snapshot.as_ref().unwrap().selection_revision().0,
            initial_selection_revision + 1,
            "a no-op bulk update does not change the selection revision"
        );
        state.rules.push(RuleDraft {
            id: state.next_rule_id,
            rule: RenameRule::Number {
                start: 1,
                step: 1,
                padding: 2,
            },
        });
        state.next_rule_id += 1;
        state.rules_revision += 1;
        state.rebuild_preview();
        let plan = state.freeze_confirmed_plan().unwrap().clone();
        assert_eq!(plan.plan().actions().len(), 2);
        let numbered_targets = plan
            .plan()
            .actions()
            .iter()
            .map(|action| action.target_path().file_name().unwrap().to_os_string())
            .collect::<Vec<_>>();
        assert_eq!(
            numbered_targets,
            [OsString::from("01alpha.txt"), OsString::from("02beta.txt")]
        );

        let recomputations = state.preview_recomputation_count;
        state.visual_sort = VisualSort {
            column: VisualSortColumn::OriginalName,
            descending: true,
        };
        state.rebuild_visible_rows();
        let sorted_names = state
            .visible_rows
            .iter()
            .map(|index| {
                state.display_rows[*index]
                    .source_name
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sorted_names,
            vec!["beta.txt".to_owned(), "alpha.txt".to_owned()]
        );
        state.filter = "beta".to_owned();
        state.rebuild_visible_rows();
        assert_eq!(state.visible_rows.len(), 1);
        for _ in 0..4 {
            let _ = run_dialog_frame(
                &mut state,
                &ctx,
                screen_input(egui::vec2(1280.0, 900.0), Vec::new()),
            );
        }
        assert_eq!(state.preview_recomputation_count, recomputations);
        assert_eq!(state.pending_intent().unwrap().plan(), plan.plan());

        state.recursive = true;
        state.start_scan(root.clone(), &coordinator);
        assert!(state.snapshot.is_none());
        assert!(state.pending_intent().is_none());
        assert!(state.selected_by_ref.is_empty());
        finish_scan(&mut state);
        assert_eq!(
            state.scan_result.as_ref().unwrap().scope(),
            crate::file_operations::RenameScanScope::Recursive
        );
        assert_eq!(state.display_rows.len(), 3);
        state.end_session();
        assert!(state.pending_intent().is_none());
        assert!(state.snapshot.is_none());
        assert!(state.scan_result.is_none());
        assert!(state.scan_generation.is_none());
        assert_eq!(state.scan_banner, ScanBanner::ChooseRoot);
        let mut actual_files = fs::read_dir(&root)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                entry.path().is_file().then(|| {
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        fs::read_to_string(entry.path()).unwrap(),
                    )
                })
            })
            .collect::<Vec<_>>();
        actual_files.sort();
        assert_eq!(expected_files, actual_files);
        assert_eq!(
            fs::read_to_string(root.join("nested/inside.txt")).unwrap(),
            "inside"
        );
        coordinator.shutdown().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn m8c_close_and_root_change_revoke_pending_scan_results() {
        let first_root = fixture_root();
        let second_root = fixture_root();
        let mut state = BatchRenameDialogState::default();
        let coordinator = RenameScanCoordinator::new().unwrap();
        state.open();
        state.start_scan(first_root.clone(), &coordinator);
        finish_scan(&mut state);
        let file_ref = state
            .scan_result
            .as_ref()
            .unwrap()
            .rows()
            .iter()
            .find(|row| row.kind() == SnapshotEntryKind::File)
            .unwrap()
            .entry_ref();
        assert!(state.set_selected(file_ref, true));
        state.rules.push(RuleDraft {
            id: state.next_rule_id,
            rule: RenameRule::Prefix("new-".to_owned()),
        });
        state.next_rule_id += 1;
        state.rules_revision += 1;
        state.rebuild_preview();
        assert!(state.freeze_confirmed_plan().is_some());
        let old_generation = state.scan_generation;
        state.start_scan(second_root.clone(), &coordinator);
        assert_ne!(state.scan_generation, old_generation);
        assert!(state.snapshot.is_none());
        assert!(state.verified_preview.is_none());
        assert!(state.pending_intent().is_none());
        assert!(state.selected_by_ref.is_empty());
        finish_scan(&mut state);
        assert_eq!(
            state.snapshot.as_ref().unwrap().canonical_root(),
            second_root
        );
        assert!(state.selected_by_ref.values().all(|selected| !selected));

        state.start_scan(first_root.clone(), &coordinator);
        state.end_session();
        state.open();
        assert!(state.scan_result.is_none());
        assert!(state.scan_generation.is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.has_pending_scan() && Instant::now() < deadline {
            state.poll_scan();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            state.snapshot.is_none(),
            "a prior session result must be discarded"
        );
        assert_eq!(state.scan_banner, ScanBanner::Cancelled);
        coordinator.shutdown().unwrap();
        fs::remove_dir_all(first_root).unwrap();
        fs::remove_dir_all(second_root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn m8d_confirmed_plan_is_frozen_and_draft_edits_revoke_apply_authority() {
        let root = fixture_root();
        let expected_files = ["alpha.txt", "beta.txt"]
            .into_iter()
            .map(|name| (name.to_owned(), fs::read(root.join(name)).unwrap()))
            .collect::<Vec<_>>();
        let mut state = BatchRenameDialogState::default();
        let coordinator = RenameScanCoordinator::new().unwrap();
        state.open();
        state.start_scan(root.clone(), &coordinator);
        finish_scan(&mut state);
        state.select_visible(true);
        state.rules.push(RuleDraft {
            id: state.next_rule_id,
            rule: RenameRule::Prefix("renamed-".to_owned()),
        });
        state.next_rule_id += 1;
        state.rules_revision += 1;
        state.rebuild_preview();

        assert!(state.pending_intent().is_none());
        let expected = state
            .verified_preview
            .as_ref()
            .unwrap()
            .verified_plan()
            .unwrap()
            .clone();
        let frozen = state.freeze_confirmed_plan().unwrap().clone();
        assert_eq!(frozen.canonical_root(), root);
        assert!(!frozen.recursive_scope());
        assert_eq!(frozen.generation(), expected.generation());
        assert_eq!(
            frozen.plan().rules_revision(),
            expected.plan().rules_revision()
        );
        let mappings = |plan: &VerifiedRenamePlan| {
            plan.plan()
                .actions()
                .iter()
                .map(|action| {
                    (
                        action.source_path().to_path_buf(),
                        action.target_path().to_path_buf(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(mappings(&frozen), mappings(&expected));
        assert_eq!(frozen.plan().actions().len(), 2);

        state.cancel_apply_confirmation();
        assert!(state.pending_intent().is_none());
        assert!(state.freeze_confirmed_plan().is_some());
        let changed_selection = state
            .scan_result
            .as_ref()
            .unwrap()
            .rows()
            .iter()
            .find(|row| row.kind() == SnapshotEntryKind::File)
            .unwrap()
            .entry_ref();
        assert!(state.set_selected(changed_selection, false));
        assert!(state.pending_intent().is_none());
        assert!(
            state
                .freeze_confirmed_plan()
                .unwrap()
                .plan()
                .actions()
                .len()
                < 2
        );

        state.end_session();
        assert!(state.pending_intent().is_none());
        for (name, contents) in expected_files {
            assert_eq!(fs::read(root.join(name)).unwrap(), contents);
        }
        coordinator.shutdown().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn m8e_launcher_route_previews_applies_and_undoes_with_guarded_recovery() {
        use crate::plugin::Plugin;

        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let fixture = app_fixture_workspace();
        let root = fixture.path().join("selected-root");
        fs::create_dir_all(&root).expect("create selected root");
        let _notes_directory = NotesDirectoryOverride::use_fixture(&fixture.path().join("notes"));
        fs::write(root.join("alpha.txt"), b"alpha bytes").expect("create alpha fixture");
        fs::write(root.join("beta.txt"), b"beta bytes").expect("create beta fixture");
        let original_bytes = file_bytes(&root);

        let ctx = egui::Context::default();
        let mut app = app_for_fixture(&ctx, fixture.path());
        let action = crate::plugins::batch_rename::BatchRenamePlugin
            .search("rename")
            .into_iter()
            .find(|action| action.action == "rename:open")
            .expect("the built-in rename plugin exposes its searchable route");
        assert!(action.args.is_none(), "the route carries no implicit root");
        assert!(action.desc.contains("guarded Apply"));
        let invocation =
            crate::commands::parse_command(action, None, crate::commands::ActivationSource::Enter)
                .expect("Enter activation parses the normal rename action");
        assert_eq!(
            invocation.command,
            crate::commands::Command::BatchRename(crate::commands::BatchRenameCommand::Open)
        );
        crate::commands::CommandBus
            .dispatch(&invocation, &mut app)
            .expect("the real launcher host opens the batch renamer");
        assert!(app.batch_rename_dialog.open);
        assert!(app.batch_rename_dialog.root_path().is_none());
        assert!(!app.batch_rename_dialog.has_pending_scan());
        assert!(app.batch_rename_coordinator.is_none());
        assert_eq!(file_bytes(&root), original_bytes);

        let open_commands = run_dialog_frame(
            &mut app.batch_rename_dialog,
            &ctx,
            screen_input(egui::vec2(1280.0, 900.0), Vec::new()),
        );
        assert!(open_commands.is_empty());
        assert!(app.batch_rename_dialog.root_path().is_none());
        assert!(app.batch_rename_coordinator.is_none());

        app.handle_batch_rename_ui_command(BatchRenameUiCommand::SelectRoot(root.clone()));
        finish_app_scan(&mut app, &ctx);
        assert_eq!(app.batch_rename_dialog.root_path(), Some(root.as_path()));
        assert_eq!(app.batch_rename_dialog.display_rows.len(), 2);
        app.batch_rename_dialog.select_visible(true);
        app.batch_rename_dialog.rules = vec![
            RuleDraft {
                id: 1,
                rule: RenameRule::Prefix("renamed-".to_owned()),
            },
            RuleDraft {
                id: 2,
                rule: RenameRule::Suffix("-copy".to_owned()),
            },
        ];
        app.batch_rename_dialog.next_rule_id = 3;
        app.batch_rename_dialog.rules_revision = 2;
        app.batch_rename_dialog.rebuild_preview();
        assert_eq!(
            app.batch_rename_dialog.extension_policy,
            ExtensionPolicy::Preserve
        );
        let original_identities = scan_identities(&app.batch_rename_dialog);
        assert_eq!(original_identities.len(), 2);
        let expected_bytes = file_bytes(&root);
        assert_eq!(expected_bytes, original_bytes);

        let runtime = app.batch_rename_runtime_snapshot();
        let preview_commands = run_dialog_frame_with_runtime(
            &mut app.batch_rename_dialog,
            &ctx,
            screen_input(egui::vec2(1280.0, 900.0), Vec::new()),
            &runtime,
        );
        assert!(preview_commands.is_empty());
        let preview = app
            .batch_rename_dialog
            .verified_preview
            .as_ref()
            .expect("the accepted explicit-root snapshot has a preview");
        assert_eq!(preview.preview().readiness(), PreviewReadiness::Ready);
        let expected_targets = preview
            .verified_plan()
            .expect("the selected preview has a verified plan")
            .plan()
            .actions()
            .iter()
            .map(|action| action.target_path().file_name().unwrap().to_os_string())
            .collect::<Vec<_>>();
        assert_eq!(
            expected_targets,
            [
                OsString::from("renamed-alpha-copy.txt"),
                OsString::from("renamed-beta-copy.txt"),
            ]
        );
        assert_eq!(file_bytes(&root), original_bytes);
        assert!(app.batch_rename_dialog.pending_intent().is_none());

        assert!(app.batch_rename_dialog.freeze_confirmed_plan().is_some());
        app.batch_rename_dialog.apply_confirmation_open = true;
        let confirmation_commands = run_dialog_frame_with_runtime(
            &mut app.batch_rename_dialog,
            &ctx,
            screen_input(egui::vec2(1280.0, 900.0), Vec::new()),
            &runtime,
        );
        assert!(confirmation_commands.is_empty());
        let confirmed_mappings = app
            .batch_rename_dialog
            .pending_intent()
            .unwrap()
            .plan()
            .actions()
            .iter()
            .map(|action| {
                (
                    action.source_ref(),
                    action.source_path().to_path_buf(),
                    action.target_path().to_path_buf(),
                )
            })
            .collect::<Vec<_>>();
        app.batch_rename_dialog.cancel_apply_confirmation();
        assert!(app.batch_rename_dialog.pending_intent().is_none());
        assert_eq!(file_bytes(&root), original_bytes);

        assert!(app.batch_rename_dialog.freeze_confirmed_plan().is_some());
        assert_eq!(
            app.batch_rename_dialog
                .pending_intent()
                .unwrap()
                .plan()
                .actions()
                .iter()
                .map(|action| {
                    (
                        action.source_ref(),
                        action.source_path().to_path_buf(),
                        action.target_path().to_path_buf(),
                    )
                })
                .collect::<Vec<_>>(),
            confirmed_mappings
        );
        app.batch_rename_dialog.apply_confirmation_open = false;
        app.handle_batch_rename_ui_command(BatchRenameUiCommand::ApplyConfirmed);

        // A second confirmation cannot enqueue another job after the frozen
        // intent was consumed. A draft edit also cannot change that job.
        app.handle_batch_rename_ui_command(BatchRenameUiCommand::ApplyConfirmed);
        assert!(
            app.batch_rename_dialog
                .status_message
                .as_deref()
                .is_some_and(|message| message.contains("no longer available"))
        );
        app.batch_rename_dialog.rules[0].rule =
            RenameRule::Prefix("edited-after-confirmation-".to_owned());
        app.batch_rename_dialog.rules_revision += 1;
        app.batch_rename_dialog.rebuild_preview();

        let applied = finish_app_operation(&mut app, &ctx);
        let applied_receipt = applied
            .receipts
            .iter()
            .find(|receipt| {
                receipt.status()
                    == crate::batch_rename_transaction::RenameTransactionStatus::Applied
            })
            .expect("the real app worker publishes the applied recovery receipt")
            .clone();
        assert_eq!(applied_receipt.entries().len(), 2);
        assert!(
            app.batch_rename_dialog
                .status_message
                .as_deref()
                .is_some_and(|message| message.contains("All confirmed filenames"))
        );
        assert_eq!(
            file_bytes(&root),
            std::collections::BTreeMap::from([
                (
                    OsString::from("renamed-alpha-copy.txt"),
                    b"alpha bytes".to_vec()
                ),
                (
                    OsString::from("renamed-beta-copy.txt"),
                    b"beta bytes".to_vec()
                ),
            ])
        );

        app.handle_batch_rename_ui_command(BatchRenameUiCommand::Rescan);
        finish_app_scan(&mut app, &ctx);
        assert_eq!(
            scan_identities(&app.batch_rename_dialog),
            std::collections::HashMap::from([
                (
                    OsString::from("renamed-alpha-copy.txt"),
                    original_identities[OsStr::new("alpha.txt")],
                ),
                (
                    OsString::from("renamed-beta-copy.txt"),
                    original_identities[OsStr::new("beta.txt")],
                ),
            ])
        );

        app.handle_batch_rename_ui_command(BatchRenameUiCommand::UndoConfirmed(
            applied_receipt.id().to_owned(),
        ));
        app.handle_batch_rename_ui_command(BatchRenameUiCommand::UndoConfirmed(
            applied_receipt.id().to_owned(),
        ));
        let undone = finish_app_operation(&mut app, &ctx);
        let undone_receipt = undone
            .receipts
            .iter()
            .find(|receipt| receipt.id() == applied_receipt.id())
            .expect("Undo updates the same receipt rather than adding another job");
        assert_eq!(
            undone_receipt.status(),
            crate::batch_rename_transaction::RenameTransactionStatus::Undone
        );
        assert_eq!(undone_receipt.remaining_undo_count(), 0);
        assert!(
            app.batch_rename_dialog
                .status_message
                .as_deref()
                .is_some_and(|message| message.contains("Undo"))
        );
        app.handle_batch_rename_ui_command(BatchRenameUiCommand::UndoConfirmed(
            applied_receipt.id().to_owned(),
        ));
        assert!(!app.batch_rename_runtime_snapshot().operation_active);
        assert!(
            app.batch_rename_dialog
                .status_message
                .as_deref()
                .is_some_and(|message| message.contains("Undo was not started"))
        );

        app.handle_batch_rename_ui_command(BatchRenameUiCommand::Rescan);
        finish_app_scan(&mut app, &ctx);
        assert_eq!(
            scan_identities(&app.batch_rename_dialog),
            original_identities
        );
        assert_eq!(file_bytes(&root), original_bytes);

        app.batch_rename_dialog.select_visible(true);
        app.batch_rename_dialog.rules = vec![RuleDraft {
            id: 4,
            rule: RenameRule::Prefix("occupied-".to_owned()),
        }];
        app.batch_rename_dialog.rules_revision += 1;
        app.batch_rename_dialog.rebuild_preview();
        assert_eq!(
            app.batch_rename_dialog
                .verified_preview
                .as_ref()
                .unwrap()
                .preview()
                .readiness(),
            PreviewReadiness::Ready
        );
        fs::write(root.join("occupied-alpha.txt"), b"unrelated occupant")
            .expect("create an unselected destination after preview");
        assert!(app.batch_rename_dialog.freeze_confirmed_plan().is_some());
        app.handle_batch_rename_ui_command(BatchRenameUiCommand::ApplyConfirmed);
        let refused = finish_app_operation(&mut app, &ctx);
        assert!(
            refused
                .last_message
                .as_deref()
                .is_some_and(|message| message.contains("destination is occupied"))
        );
        assert!(
            app.batch_rename_dialog
                .status_message
                .as_deref()
                .is_some_and(|message| message.contains("Refresh the preview"))
        );
        assert_eq!(fs::read(root.join("alpha.txt")).unwrap(), b"alpha bytes");
        assert_eq!(fs::read(root.join("beta.txt")).unwrap(), b"beta bytes");
        assert_eq!(
            fs::read(root.join("occupied-alpha.txt")).unwrap(),
            b"unrelated occupant"
        );
        assert_eq!(
            refused.receipts.len(),
            1,
            "a refused no-move attempt does not replace or duplicate the Undo receipt"
        );
        assert_eq!(
            refused.receipts[0].status(),
            crate::batch_rename_transaction::RenameTransactionStatus::Undone
        );

        app.shutdown_batch_rename_scanner();
        drop(app);
    }

    #[test]
    fn m8c_path_presentation_bounds_unicode_without_changing_source_text() {
        let path = "C:\\Users\\测试\\a very long directory\\filename.txt";
        let shown = truncate_middle(path, 24);
        assert!(shown.chars().count() <= 24);
        assert!(shown.starts_with("C:\\Users"));
        assert!(shown.ends_with("lename.txt"));
        assert!(path.chars().count() > shown.chars().count());
    }
}
