//! Resizable Hot Folder rule manager. This module owns only drafts and view
//! state; every persistent or runtime transition is returned as a typed
//! request to the app-owned driver.

use crate::hotfolder::{
    ActionCapability, HotFolderEventKind, HotFolderExecutionAction,
    HotFolderExecutionPhase, HotFolderExecutionReceipt, HotFolderRule, HotFolderTerminalStatus,
    RuleAction, RuleId, RuleTrigger, StrongActionConfirmation,
};
use crate::gui::hotfolder_runtime::{
    HotFolderCapabilityInputs, HotFolderDriverSnapshot, HotFolderEnablePreview,
    evaluate_sample_conditions,
};
use crate::mkmacro::MkMacroDocument;
use crate::universal_actions::ResolvedActionTarget;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::SystemTime;

const HISTORY_ROWS: usize = 128;

#[derive(Clone)]
pub(crate) enum HotFolderDialogAction {
    EnsureLoaded,
    SaveNew {
        expected_revision: u64,
        rule: HotFolderRule,
    },
    SaveEdit {
        expected_revision: u64,
        rule: HotFolderRule,
    },
    Delete {
        expected_revision: u64,
        rule: HotFolderRule,
    },
    UndoDelete(HotFolderRule),
    PrepareEnable {
        expected_revision: u64,
        rule_id: RuleId,
    },
    ConfirmEnable {
        preview_id: u64,
        confirmation: StrongActionConfirmation,
    },
    CancelEnable(u64),
    Disable {
        expected_revision: u64,
        rule_id: RuleId,
    },
    Pause,
    Resume,
    ClearHistory,
    TestRule {
        editor_id: u128,
        rule: HotFolderRule,
        sample: PathBuf,
        event: HotFolderEventKind,
        draft_revision: u64,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingKind {
    Load,
    Save,
    Delete,
    PrepareEnable,
    ConfirmEnable,
    Disable,
    Pause,
    Resume,
    ClearHistory,
    UndoDelete,
    CancelEnable,
}

struct PendingRequest {
    request_id: u64,
    kind: PendingKind,
    undo_rule: Option<HotFolderRule>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditMode {
    New,
    Existing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionChoice {
    Copy,
    Move,
    Rename,
    RegisteredAction,
    MkMacro,
}

impl ActionChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Rename => "Rename",
            Self::RegisteredAction => "Registered launcher action",
            Self::MkMacro => "MkMacro",
        }
    }

    fn from_action(action: &RuleAction) -> Self {
        match action {
            RuleAction::Copy { .. } => Self::Copy,
            RuleAction::Move { .. } => Self::Move,
            RuleAction::Rename { .. } => Self::Rename,
            RuleAction::RegisteredLauncherAction { .. } => Self::RegisteredAction,
            RuleAction::MkMacro { .. } => Self::MkMacro,
        }
    }
}

struct RuleEditor {
    editor_id: u128,
    mode: EditMode,
    expected_revision: u64,
    draft: HotFolderRule,
    glob_text: String,
    extensions_text: String,
    action_choice: ActionChoice,
    picker_open: bool,
    sample_path: Option<PathBuf>,
    sample_event: HotFolderEventKind,
    draft_revision: u64,
    validation_error: Option<String>,
}

impl RuleEditor {
    fn new(
        editor_id: u128,
        mode: EditMode,
        expected_revision: u64,
        draft: HotFolderRule,
    ) -> Self {
        Self {
            editor_id,
            mode,
            expected_revision,
            glob_text: draft.filter.glob.clone().unwrap_or_default(),
            extensions_text: draft.filter.extensions.join(", "),
            action_choice: ActionChoice::from_action(&draft.action),
            draft,
            picker_open: false,
            sample_path: None,
            sample_event: HotFolderEventKind::Created,
            draft_revision: 0,
            validation_error: None,
        }
    }

    fn update_filter_from_text(&mut self) {
        let glob = self.glob_text.trim();
        self.draft.filter.glob = (!glob.is_empty()).then(|| glob.to_owned());
        self.draft.filter.extensions = self
            .extensions_text
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
    }

    fn current_action_matches_choice(&self) -> bool {
        ActionChoice::from_action(&self.draft.action) == self.action_choice
    }

    fn validate(&mut self) {
        self.update_filter_from_text();
        self.validation_error = self
            .draft
            .validate()
            .err()
            .map(str::to_owned)
            .or_else(|| {
                (!self.current_action_matches_choice())
                    .then(|| "Choose an exact current action or macro before saving".into())
            });
    }

    fn changed(&mut self) {
        self.draft_revision = self.draft_revision.wrapping_add(1);
        self.validation_error = None;
    }
}

struct SampleTestJob {
    editor_id: u128,
    draft_revision: u64,
    receiver: mpsc::Receiver<SampleTestResult>,
}

#[derive(Clone)]
struct SampleTestResult {
    editor_id: u128,
    draft_revision: u64,
    sample: PathBuf,
    size: Option<u64>,
    modified: Option<SystemTime>,
    condition: Result<(), String>,
    capability: ActionCapability,
    plan: Option<String>,
}

/// UI state for the Hot Folder manager. It deliberately contains no Store,
/// watcher or executor handle.
pub(crate) struct HotFolderDialog {
    pub(crate) open: bool,
    editor: Option<RuleEditor>,
    picker_targets: Option<Vec<ResolvedActionTarget>>,
    picker_macros: Option<MkMacroDocument>,
    pending: Option<PendingRequest>,
    error: Option<String>,
    notice: Option<String>,
    delete_confirmation: Option<(u64, HotFolderRule)>,
    undo_delete: Option<HotFolderRule>,
    clear_history_confirmation: bool,
    show_full_paths: bool,
    test_job: Option<SampleTestJob>,
    test_result: Option<SampleTestResult>,
    next_editor_id: u128,
}

impl Default for HotFolderDialog {
    fn default() -> Self {
        Self {
            open: false,
            editor: None,
            picker_targets: None,
            picker_macros: None,
            pending: None,
            error: None,
            notice: None,
            delete_confirmation: None,
            undo_delete: None,
            clear_history_confirmation: false,
            show_full_paths: false,
            test_job: None,
            test_result: None,
            next_editor_id: 1,
        }
    }
}

impl HotFolderDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn needs_picker_snapshot(&self) -> bool {
        self.open
            && self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.picker_open)
            && (self.picker_targets.is_none() || self.picker_macros.is_none())
    }

    pub(crate) fn needs_runtime_poll(
        &self,
        snapshot: &HotFolderDriverSnapshot,
        root_visible: bool,
    ) -> bool {
        if self.open && root_visible {
            return true;
        }
        self.pending.as_ref().is_some_and(|pending| {
            pending.request_id != 0
                && !snapshot
                    .acknowledgements
                    .iter()
                    .any(|ack| ack.request_id == pending.request_id)
        })
    }

    pub(crate) fn set_picker_snapshot(
        &mut self,
        targets: Vec<ResolvedActionTarget>,
        macros: MkMacroDocument,
    ) {
        self.picker_targets = Some(targets);
        self.picker_macros = Some(macros);
    }

    pub(crate) fn note_request(
        &mut self,
        request_id: u64,
        kind: PendingKind,
        undo_rule: Option<HotFolderRule>,
    ) {
        self.pending = Some(PendingRequest {
            request_id,
            kind,
            undo_rule,
        });
        self.error = None;
        self.notice = None;
    }

    pub(crate) fn submission_failed(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.pending = None;
    }

    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        snapshot: &HotFolderDriverSnapshot,
    ) -> Vec<HotFolderDialogAction> {
        let mut actions = Vec::new();
        self.poll_sample_test();
        self.poll_acknowledgement(snapshot);
        if !snapshot.loaded && self.open && self.pending.is_none() {
            actions.push(HotFolderDialogAction::EnsureLoaded);
            self.pending = Some(PendingRequest {
                request_id: 0,
                kind: PendingKind::Load,
                undo_rule: None,
            });
        }

        let preview = snapshot
            .enable_previews
            .iter()
            .find(|preview| {
                self.editor
                    .as_ref()
                    .is_none_or(|editor| editor.draft.id == preview.rule.id)
            })
            .cloned();
        let was_open = self.open;
        let mut window_open = self.open;
        egui::Window::new("Hot Folder Manager")
            .id(egui::Id::new("hotfolder_manager_window"))
            .open(&mut window_open)
            .resizable(true)
            .default_size([940.0, 660.0])
            .min_width(680.0)
            .min_height(420.0)
            .show(ctx, |ui| {
                if !snapshot.plugin_enabled {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Hot Folder is disabled in Plugins. Stored rule intent is retained.",
                    );
                } else if snapshot.paused {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Paused — no new file events are admitted.",
                    );
                }
                if let Some(error) = snapshot.error.as_deref() {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                if let Some(error) = self.error.as_deref() {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                if let Some(notice) = self.notice.as_deref() {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, notice);
                }
                if let Some(pending) = &self.pending
                    && pending.request_id != 0
                {
                    ui.label("Applying control; waiting for the runtime acknowledgement…");
                }

                ui.horizontal(|ui| {
                    if snapshot.plugin_enabled {
                        if snapshot.paused {
                            if ui.button("Resume").clicked() {
                                actions.push(HotFolderDialogAction::Resume);
                            }
                        } else if ui.button("Pause").clicked() {
                            actions.push(HotFolderDialogAction::Pause);
                        }
                    }
                    if ui.button("New Rule").clicked() && self.pending.is_none() {
                        self.begin_new(snapshot.store.as_ref().map_or(0, |store| store.revision));
                    }
                    if let Some(rule) = self.undo_delete.as_ref()
                        && ui.button("Undo Delete").clicked()
                    {
                        actions.push(HotFolderDialogAction::UndoDelete(rule.clone()));
                    }
                });

                if let Some(store) = snapshot.store.as_ref() {
                    ui.label(format!(
                        "Rules: {} · quarantined rows: {} · revision {}",
                        store.rules.len(),
                        store.quarantined_rows.len(),
                        store.revision
                    ));
                    if let Some(reason) = store.write_block_reason() {
                        ui.colored_label(egui::Color32::LIGHT_RED, reason);
                    }
                    if !store.quarantined_rows.is_empty() {
                        egui::CollapsingHeader::new("Quarantine diagnostics (raw rows preserved)")
                            .show(ui, |ui| {
                                for row in &store.quarantined_rows {
                                    ui.label(row.diagnostic.as_str());
                                }
                            });
                    }
                    ui.separator();
                    if self.editor.is_some() {
                        self.render_editor(ui, store.revision, &mut actions);
                    } else {
                        self.render_rules(ui, snapshot, &mut actions);
                    }
                } else {
                    ui.label("Loading stored rules…");
                }

                ui.separator();
                self.render_runtime_status(ui, snapshot);
                ui.separator();
                self.render_history(ui, snapshot);

                if let Some(preview) = preview.as_ref()
                    && self.editor.is_none()
                {
                    ui.separator();
                    self.render_strong_confirmation(ui, preview, &mut actions);
                }
                if let Some((revision, rule)) = self.delete_confirmation.clone() {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Delete rule ‘{}’? It will be disabled and removed from the stored list.", rule.name),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Delete Rule").clicked() {
                            actions.push(HotFolderDialogAction::Delete {
                                expected_revision: revision,
                                rule: rule.clone(),
                            });
                            self.delete_confirmation = None;
                        }
                        if ui.button("Cancel Delete").clicked() {
                            self.delete_confirmation = None;
                        }
                    });
                }
                if self.clear_history_confirmation {
                    ui.separator();
                    ui.label("Clear terminal history from display and durable history. Recovery and dedupe records stay intact.");
                    ui.horizontal(|ui| {
                        if ui.button("Clear Terminal History").clicked() {
                            actions.push(HotFolderDialogAction::ClearHistory);
                            self.clear_history_confirmation = false;
                        }
                        if ui.button("Keep History").clicked() {
                            self.clear_history_confirmation = false;
                        }
                    });
                }
            });
        self.open = window_open;

        let cancellation_pending = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.kind == PendingKind::CancelEnable);
        if (was_open && !self.open || self.editor.is_some())
            && !cancellation_pending
            && let Some(preview) = preview
        {
            actions.push(HotFolderDialogAction::CancelEnable(preview.request_id));
            self.picker_targets = None;
            self.picker_macros = None;
        }
        if self.editor.is_none() || !self.open {
            self.picker_targets = None;
            self.picker_macros = None;
        }
        actions
    }

    fn poll_acknowledgement(&mut self, snapshot: &HotFolderDriverSnapshot) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        if pending.request_id == 0 {
            return;
        }
        let Some(ack) = snapshot
            .acknowledgements
            .iter()
            .find(|ack| ack.request_id == pending.request_id)
        else {
            return;
        };
        let pending = self.pending.take().expect("pending request was inspected");
        match &ack.result {
            Ok(()) => {
                self.error = None;
                self.notice = Some("Control acknowledged by the Hot Folder runtime.".into());
                match pending.kind {
                    PendingKind::Save => {
                        self.editor = None;
                        self.retire_sample_test();
                        self.notice = Some("Rule saved disabled. Enable it explicitly after review.".into());
                    }
                    PendingKind::Delete => {
                        self.undo_delete = pending.undo_rule;
                    }
                    PendingKind::UndoDelete => {
                        self.undo_delete = None;
                    }
                    PendingKind::CancelEnable => {}
                    PendingKind::ClearHistory => {
                        self.notice = Some("Terminal history cleared; dedupe and recovery records were retained.".into());
                    }
                    PendingKind::Load
                    | PendingKind::PrepareEnable
                    | PendingKind::ConfirmEnable
                    | PendingKind::Disable
                    | PendingKind::Pause
                    | PendingKind::Resume => {}
                }
            }
            Err(error) => self.error = Some(error.clone()),
        }
    }

    fn begin_new(&mut self, revision: u64) {
        let mut rule = HotFolderRule::new(
            "",
            PathBuf::new(),
            RuleAction::Copy {
                destination: PathBuf::new(),
            },
        );
        rule.enabled = false;
        self.editor = Some(RuleEditor::new(self.allocate_editor_id(), EditMode::New, revision, rule));
        self.picker_targets = None;
        self.picker_macros = None;
        self.retire_sample_test();
        self.error = None;
    }

    fn begin_edit(&mut self, revision: u64, rule: HotFolderRule) {
        self.editor = Some(RuleEditor::new(self.allocate_editor_id(), EditMode::Existing, revision, rule));
        self.picker_targets = None;
        self.picker_macros = None;
        self.retire_sample_test();
        self.error = None;
    }

    fn allocate_editor_id(&mut self) -> u128 {
        let editor_id = self.next_editor_id;
        self.next_editor_id = self
            .next_editor_id
            .checked_add(1)
            .expect("Hot Folder editor identity space is exhausted");
        editor_id
    }

    fn retire_sample_test(&mut self) {
        self.test_job = None;
        self.test_result = None;
    }

    fn render_rules(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &HotFolderDriverSnapshot,
        actions: &mut Vec<HotFolderDialogAction>,
    ) {
        let Some(store) = snapshot.store.as_ref() else {
            return;
        };
        if store.rules.is_empty() {
            ui.label("No rules yet. Add one with an explicit watched directory and action.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("hotfolder_rules_list")
            .max_height(220.0)
            .show(ui, |ui| {
                for rule in &store.rules {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(rule.name.as_str());
                            let state = if !rule.enabled {
                                "Disabled"
                            } else if snapshot.paused {
                                "Paused"
                            } else {
                                "Enabled"
                            };
                            ui.label(state);
                            ui.label(action_summary(&rule.action));
                        });
                        ui.label(format!("Watching: {}", rule.watched_root.display()));
                        ui.label(rule_scope_summary(rule));
                        ui.horizontal(|ui| {
                            if ui.button("Edit").clicked() && self.pending.is_none() {
                                self.begin_edit(store.revision, rule.clone());
                            }
                            if snapshot.plugin_enabled && rule.enabled {
                                if ui.button("Disable").clicked() {
                                    actions.push(HotFolderDialogAction::Disable {
                                        expected_revision: store.revision,
                                        rule_id: rule.id.clone(),
                                    });
                                }
                            } else if snapshot.plugin_enabled
                                && !rule.enabled
                                && !snapshot.paused
                                && snapshot.enable_previews.is_empty()
                                && ui.button("Enable…").clicked()
                            {
                                actions.push(HotFolderDialogAction::PrepareEnable {
                                    expected_revision: store.revision,
                                    rule_id: rule.id.clone(),
                                });
                            }
                            if ui.button("Delete…").clicked() {
                                self.delete_confirmation = Some((store.revision, rule.clone()));
                            }
                        });
                    });
                }
            });
    }

    fn render_editor(
        &mut self,
        ui: &mut egui::Ui,
        current_revision: u64,
        actions: &mut Vec<HotFolderDialogAction>,
    ) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let old_draft = editor.draft.clone();
        let old_glob = editor.glob_text.clone();
        let old_extensions = editor.extensions_text.clone();
        let old_sample = editor.sample_path.clone();
        let old_event = editor.sample_event;
        let mut requested_picker = None;
        let mut cancel_draft = false;
        ui.heading(match editor.mode {
            EditMode::New => "New disabled rule",
            EditMode::Existing => "Edit rule draft",
        });
        if editor.expected_revision != current_revision {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Stored rules changed while this draft was open. Save is disabled; cancel and reopen the current rule.",
            );
        }
        egui::Grid::new("hotfolder_rule_editor_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Rule name");
                ui.text_edit_singleline(&mut editor.draft.name);
                ui.end_row();

                ui.label("Watched directory");
                ui.horizontal(|ui| {
                    ui.label(editor.draft.watched_root.display().to_string());
                    if ui.button("Choose directory…").clicked() {
                        requested_picker = Some(0_u8);
                    }
                });
                ui.end_row();

                ui.label("Trigger");
                egui::ComboBox::from_id_salt("hotfolder_trigger")
                    .selected_text(match editor.draft.trigger {
                        RuleTrigger::NewFile => "New file",
                        RuleTrigger::ModifiedFile => "Modified file",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.draft.trigger, RuleTrigger::NewFile, "New file");
                        ui.selectable_value(&mut editor.draft.trigger, RuleTrigger::ModifiedFile, "Modified file");
                    });
                ui.end_row();

                ui.label("Filename glob (* and ?)");
                ui.text_edit_singleline(&mut editor.glob_text);
                ui.end_row();

                ui.label("Extensions (comma separated)");
                ui.text_edit_singleline(&mut editor.extensions_text);
                ui.end_row();

                ui.label("Scope");
                ui.vertical(|ui| {
                    ui.checkbox(&mut editor.draft.include_subfolders, "Include subfolders");
                    ui.checkbox(&mut editor.draft.include_temporary_files, "Include .part and .crdownload files");
                });
                ui.end_row();

                ui.label("Settle policy");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut editor.draft.settle.sample_interval_ms).range(50..=5000).suffix(" ms interval"));
                    ui.add(egui::DragValue::new(&mut editor.draft.settle.stable_observations).range(2..=10).suffix(" stable observations"));
                    ui.add(egui::DragValue::new(&mut editor.draft.settle.timeout_ms).range(100..=30000).suffix(" ms timeout"));
                });
                ui.end_row();

                ui.label("Action");
                ui.horizontal_wrapped(|ui| {
                    for choice in [ActionChoice::Copy, ActionChoice::Move, ActionChoice::Rename, ActionChoice::RegisteredAction, ActionChoice::MkMacro] {
                        if ui.selectable_label(editor.action_choice == choice, choice.label()).clicked() {
                            select_action_choice(&mut editor, choice);
                        }
                    }
                });
                ui.end_row();

                match (&mut editor.draft.action, editor.action_choice) {
                    (RuleAction::Copy { destination } | RuleAction::Move { destination }, ActionChoice::Copy | ActionChoice::Move) => {
                        ui.label("Destination directory");
                        ui.horizontal(|ui| {
                            ui.label(destination.display().to_string());
                            if ui.button("Choose directory…").clicked() { requested_picker = Some(1); }
                        });
                        ui.end_row();
                    }
                    (RuleAction::Rename { prefix, suffix }, ActionChoice::Rename) => {
                        ui.label("Rename prefix");
                        ui.text_edit_singleline(prefix);
                        ui.end_row();
                        ui.label("Rename suffix");
                        ui.text_edit_singleline(suffix);
                        ui.end_row();
                    }
                    (RuleAction::RegisteredLauncherAction { .. }, ActionChoice::RegisteredAction)
                    | (RuleAction::MkMacro { .. }, ActionChoice::MkMacro) => {
                        ui.label("Exact action");
                        ui.horizontal(|ui| {
                            ui.label(action_summary(&editor.draft.action));
                            if ui.button("Choose exact current target…").clicked() {
                                editor.picker_open = !editor.picker_open;
                                if !editor.picker_open {
                                    self.picker_targets = None;
                                    self.picker_macros = None;
                                }
                            }
                        });
                        ui.end_row();
                    }
                    _ => {
                        ui.label("Exact action");
                        ui.colored_label(egui::Color32::YELLOW, "Choose a current registered action or macro.");
                        ui.end_row();
                    }
                }
            });

        if editor.picker_open {
            render_action_picker(
                ui,
                &mut editor,
                self.picker_targets.as_deref().unwrap_or_default(),
                self.picker_macros.as_ref(),
            );
            if !editor.picker_open {
                self.picker_targets = None;
                self.picker_macros = None;
            }
        }
        if let Some(field) = requested_picker {
            match field {
                0 => {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        editor.draft.watched_root = path;
                    }
                }
                1 => {
                    if let Some(path) = rfd::FileDialog::new().pick_folder()
                        && let RuleAction::Copy { destination } | RuleAction::Move { destination } = &mut editor.draft.action
                    {
                        *destination = path;
                    }
                }
                _ => {}
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Sample file (read-only metadata only)");
            if ui.button("Choose sample file…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                editor.sample_path = Some(path);
            }
            ui.label(editor.sample_path.as_ref().map_or_else(|| "No sample selected".into(), |path| path.display().to_string()));
        });
        egui::ComboBox::from_id_salt("hotfolder_test_event")
            .selected_text(event_label(editor.sample_event))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut editor.sample_event, HotFolderEventKind::Created, "Created");
                ui.selectable_value(&mut editor.sample_event, HotFolderEventKind::Modified, "Modified");
                ui.selectable_value(&mut editor.sample_event, HotFolderEventKind::RenamedTo, "Renamed to");
            });

        editor.validate();
        if let Some(error) = editor.validation_error.as_deref() {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Validation: {error}"));
        } else {
            ui.colored_label(egui::Color32::LIGHT_GREEN, "Draft is structurally valid; current capability is checked again on Enable.");
        }
        if old_draft != editor.draft
            || old_glob != editor.glob_text
            || old_extensions != editor.extensions_text
            || old_sample != editor.sample_path
            || old_event != editor.sample_event
        {
            editor.changed();
            self.test_result = None;
        }

        ui.horizontal(|ui| {
            let saving = self.pending.is_some();
            if ui.add_enabled(!saving && editor.validation_error.is_none() && editor.expected_revision == current_revision, egui::Button::new("Save Disabled Rule")).clicked() {
                let mut rule = editor.draft.clone();
                rule.enabled = false;
                actions.push(match editor.mode {
                    EditMode::New => HotFolderDialogAction::SaveNew { expected_revision: editor.expected_revision, rule },
                    EditMode::Existing => HotFolderDialogAction::SaveEdit { expected_revision: editor.expected_revision, rule },
                });
            }
            if ui.add_enabled(!saving && editor.sample_path.is_some(), egui::Button::new("Test Rule (zero effects)")).clicked()
                && let Some(action) = sample_test_action(&editor)
            {
                actions.push(action);
            }
            if ui.add_enabled(!saving, egui::Button::new("Cancel Draft")).clicked() {
                cancel_draft = true;
                self.picker_targets = None;
                self.picker_macros = None;
                self.test_result = None;
            }
        });
        if let Some(result) = self.test_result.as_ref()
            && editor.draft_revision == result.draft_revision
        {
            self.render_test_result(ui, result);
        }
        if !cancel_draft {
            self.editor = Some(editor);
        } else {
            self.editor = None;
            self.retire_sample_test();
        }
    }

    fn render_test_result(&self, ui: &mut egui::Ui, result: &SampleTestResult) {
        ui.group(|ui| {
            ui.strong("Pure sample evaluation");
            ui.label(format!("Sample: {}", result.sample.file_name().unwrap_or_default().to_string_lossy()));
            if let Some(size) = result.size {
                ui.label(format!("Read-only metadata: {size} bytes{}", result.modified.map_or_else(String::new, |time| format!(" · {}", format_time(time)))));
            }
            match &result.condition {
                Ok(()) => {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Conditions: match");
                    if let Some(plan) = result.plan.as_deref() {
                        ui.label(format!("Planned action: {plan}"));
                    }
                }
                Err(reason) => {
                    ui.colored_label(egui::Color32::YELLOW, format!("Conditions: no match — {reason}"));
                }
            }
            ui.label(format!("Capability: {}", capability_label(&result.capability)));
            ui.small("This sample evaluation did not enter the live watcher or execution history.");
        });
    }

    fn render_runtime_status(&self, ui: &mut egui::Ui, snapshot: &HotFolderDriverSnapshot) {
        ui.horizontal(|ui| {
            ui.strong("Runtime");
            if snapshot.running {
                ui.label(if snapshot.paused { "Paused" } else { "Running in the background" });
            } else if snapshot.plugin_enabled {
                ui.label("Starting or reconnecting");
            } else {
                ui.label("Stopped");
            }
            if let Some(runtime) = snapshot.runtime.as_ref() {
                ui.label(format!(
                    "Roots {} · rules {} · settling {} · ready {}",
                    runtime.active_roots,
                    runtime.active_rules,
                    runtime.pending_files,
                    runtime.ready_candidates
                ));
            }
            if let Some(operation_id) = snapshot.execution_progress.operation_id {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    if snapshot.execution_progress.cancellation_requested {
                        format!("In-flight action #{operation_id} — cancellation requested; awaiting terminal receipt")
                    } else {
                        format!("In-flight action #{operation_id} — awaiting terminal receipt")
                    },
                );
            } else if snapshot.execution.as_ref().is_some_and(|execution| execution.active) {
                ui.colored_label(egui::Color32::YELLOW, "In-flight action — awaiting a terminal receipt");
            }
            if let Some(execution) = snapshot.execution.as_ref()
                && execution.recovery_required > 0
            {
                ui.colored_label(egui::Color32::YELLOW, format!("{} recovery record(s) retained", execution.recovery_required));
            }
        });
        if let Some(runtime) = snapshot.runtime.as_ref() {
            for diagnostic in runtime.diagnostics.iter().take(8) {
                ui.small(format!("Watcher diagnostic: {:?}{}", diagnostic.kind, diagnostic.rule_id.as_ref().map_or_else(String::new, |id| format!(" · rule {}", id.as_str()))));
            }
        }
        if let Some(execution) = snapshot.execution.as_ref()
            && let Some(error) = execution.journal_error.as_deref()
        {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Execution journal: {error}"));
        }
    }

    fn render_history(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &HotFolderDriverSnapshot,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Terminal history");
            ui.checkbox(&mut self.show_full_paths, "Show full paths");
            if ui.add_enabled(snapshot.execution.is_some(), egui::Button::new("Clear History…")).clicked() {
                self.clear_history_confirmation = true;
            }
        });
        let Some(execution) = snapshot.execution.as_ref() else {
            ui.label("History is available when Hot Folder is enabled.");
            return;
        };
        if let Some(operation_id) = snapshot.execution_progress.operation_id {
            ui.colored_label(
                egui::Color32::YELLOW,
                if snapshot.execution_progress.cancellation_requested {
                    format!("Operation #{operation_id} is still active; cancellation was requested and no terminal receipt has arrived.")
                } else {
                    format!("Operation #{operation_id} is in flight; no terminal receipt has arrived.")
                },
            );
        }
        if execution.receipts.is_empty() {
            ui.label("No terminal receipts yet. Settled or queued files are not shown as successful runs.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("hotfolder_terminal_history")
            .max_height(180.0)
            .show(ui, |ui| {
                for receipt in execution.receipts.iter().rev().take(HISTORY_ROWS) {
                    self.render_receipt(ui, receipt);
                }
            });
    }

    fn render_receipt(&self, ui: &mut egui::Ui, receipt: &HotFolderExecutionReceipt) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("{} · {}", status_label(receipt.status), action_label(receipt.action)));
                ui.label(format_unix_nanos(receipt.started_unix_nanos));
                ui.label(format!("{} ms", receipt.duration_ms));
                ui.label(if receipt.durable { "Durable terminal receipt" } else { "Terminal receipt not durable" });
            });
            let source = if self.show_full_paths {
                receipt.source_path.display().to_string()
            } else {
                basename(&receipt.source_path)
            };
            ui.label(format!("Rule {} · input {source}", receipt.rule_id.as_str()));
            if let Some(reason) = receipt.reason.as_deref() {
                ui.label(format!("Reason: {reason}"));
            }
            if receipt.status == HotFolderTerminalStatus::Partial {
                let partial = match (receipt.action, receipt.phase, receipt.outputs.is_empty()) {
                    (HotFolderExecutionAction::Move, HotFolderExecutionPhase::SourceRemoval, false) => "Destination was created; the source was retained after removal failed.",
                    (HotFolderExecutionAction::Copy, _, false) => "A destination output was retained; the input remains available.",
                    _ => "The operation stopped after a mutation boundary; inspect the recorded outputs and recovery status.",
                };
                ui.colored_label(egui::Color32::YELLOW, partial);
            }
            for output in receipt.outputs.iter().take(8) {
                let path = if self.show_full_paths {
                    output.path.display().to_string()
                } else {
                    basename(&output.path)
                };
                ui.small(format!("Output {:?}: {path}", output.kind));
            }
            if let Some(run_id) = receipt.external_run_id {
                ui.small(format!("Owned MkMacro run ID {run_id}"));
            }
        });
    }

    fn render_strong_confirmation(
        &self,
        ui: &mut egui::Ui,
        preview: &HotFolderEnablePreview,
        actions: &mut Vec<HotFolderDialogAction>,
    ) {
        ui.group(|ui| {
            ui.colored_label(egui::Color32::YELLOW, "Confirm this exact unattended action");
            ui.label(format!("Rule: {}", preview.rule.name));
            ui.label(format!("Watched root: {}", preview.rule.watched_root.display()));
            ui.label(format!("Action: {}", action_summary(&preview.rule.action)));
            ui.label(rule_scope_summary(&preview.rule));
            ui.label(format!("This authorization is bound to rule revision {} and the selected target configuration fingerprint.", preview.resulting_revision));
            if let RuleAction::Move { destination } | RuleAction::Copy { destination } = &preview.rule.action {
                ui.label(format!("Destination: {}", destination.display()));
            }
            if let ActionCapability::Supported { enablement } = &preview.capability {
                ui.label(format!("Capability: {enablement:?}"));
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.pending.is_none(),
                        egui::Button::new("Confirm Enable"),
                    )
                    .clicked()
                {
                    let confirmation = StrongActionConfirmation::from_explicit_confirmation(
                        preview.rule.id.clone(),
                        preview.resulting_revision,
                        preview.fingerprint.clone(),
                    );
                    actions.push(HotFolderDialogAction::ConfirmEnable {
                        preview_id: preview.request_id,
                        confirmation,
                    });
                }
                if ui
                    .add_enabled(
                        self.pending.is_none(),
                        egui::Button::new("Cancel Enable"),
                    )
                    .clicked()
                {
                    actions.push(HotFolderDialogAction::CancelEnable(preview.request_id));
                }
            });
        });
    }

    fn start_sample_test(
        &mut self,
        editor_id: u128,
        rule: HotFolderRule,
        sample: PathBuf,
        event: HotFolderEventKind,
        draft_revision: u64,
        inputs: HotFolderCapabilityInputs,
        adapter: Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter>,
    ) {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.test_job = Some(SampleTestJob {
            editor_id,
            draft_revision,
            receiver,
        });
        self.test_result = None;
        thread::spawn(move || {
            let result = evaluate_sample_test(editor_id, rule, sample, event, draft_revision, &inputs, adapter.as_ref());
            let _ = sender.send(result);
        });
    }

    fn poll_sample_test(&mut self) {
        let Some(((editor_id, generation), received)) = self
            .test_job
            .as_ref()
            .map(|job| ((job.editor_id, job.draft_revision), job.receiver.try_recv()))
        else {
            return;
        };
        match received {
            Ok(result) => {
                self.test_job = None;
                if self.editor.as_ref().is_some_and(|editor| {
                    editor.editor_id == editor_id
                        && editor.draft_revision == generation
                        && result.editor_id == editor_id
                        && result.draft_revision == generation
                }) {
                    self.test_result = Some(result);
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => self.test_job = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

impl HotFolderDialog {
    pub(crate) fn start_sample_test_from_action(
        &mut self,
        action: HotFolderDialogAction,
        inputs: HotFolderCapabilityInputs,
        adapter: Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter>,
    ) -> Option<HotFolderDialogAction> {
        match action {
            HotFolderDialogAction::TestRule {
                editor_id,
                rule,
                sample,
                event,
                draft_revision,
            } => {
                if self.editor.as_ref().is_some_and(|editor| {
                    editor.editor_id == editor_id && editor.draft_revision == draft_revision
                }) {
                    self.start_sample_test(
                        editor_id,
                        rule,
                        sample,
                        event,
                        draft_revision,
                        inputs,
                        adapter,
                    );
                }
                None
            }
            action => Some(action),
        }
    }
}

fn evaluate_sample_test(
    editor_id: u128,
    rule: HotFolderRule,
    sample: PathBuf,
    event: HotFolderEventKind,
    draft_revision: u64,
    inputs: &HotFolderCapabilityInputs,
    adapter: &dyn crate::hotfolder::HotFolderExternalActionAdapter,
) -> SampleTestResult {
    let root = fs_canonicalize(&rule.watched_root);
    let sample_path = fs_canonicalize(&sample);
    let metadata = sample_path.as_ref().ok().and_then(|path| std::fs::metadata(path).ok());
    let relative = root
        .as_ref()
        .ok()
        .and_then(|root| sample_path.as_ref().ok().and_then(|path| crate::hotfolder::relative_path_beneath(root, path)));
    let condition = match (&root, &sample_path, &metadata, &relative) {
        (Err(_), _, _, _) => Err("watched root could not be opened for read-only metadata".into()),
        (_, Err(_), _, _) => Err("sample file could not be opened for read-only metadata".into()),
        (_, _, None, _) => Err("sample metadata is unavailable".into()),
        (_, _, Some(metadata), _) if !metadata.is_file() => Err("sample path is not a file".into()),
        (_, _, _, None) => Err("sample file is outside the watched root".into()),
        (_, _, _, Some(relative)) => evaluate_sample_conditions(&rule, event, relative)
            .map_err(str::to_owned),
    };
    let capability = inputs.evaluate_rule(&rule, adapter);
    let plan = condition
        .is_ok()
        .then(|| planned_action_summary(&rule, &sample))
        .flatten();
    SampleTestResult {
        editor_id,
        draft_revision,
        sample,
        size: metadata.as_ref().map(std::fs::Metadata::len),
        modified: metadata.and_then(|metadata| metadata.modified().ok()),
        condition,
        capability,
        plan,
    }
}

fn sample_test_action(editor: &RuleEditor) -> Option<HotFolderDialogAction> {
    Some(HotFolderDialogAction::TestRule {
        editor_id: editor.editor_id,
        rule: editor.draft.clone(),
        sample: editor.sample_path.clone()?,
        event: editor.sample_event,
        draft_revision: editor.draft_revision,
    })
}

fn fs_canonicalize(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|_| "path is unavailable".into())
}

fn planned_action_summary(rule: &HotFolderRule, sample: &Path) -> Option<String> {
    let filename = sample.file_name()?;
    Some(match &rule.action {
        RuleAction::Copy { destination } => format!("Copy to {}", destination.join(filename).display()),
        RuleAction::Move { destination } => format!("Move to {}", destination.join(filename).display()),
        RuleAction::Rename { prefix, suffix } => match crate::batch_rename::transform_filename_with_prefix_suffix(filename, prefix, suffix) {
            Ok(next) => format!("Rename to {}", sample.with_file_name(next).display()),
            Err(_) => "Rename plan is invalid".into(),
        },
        RuleAction::RegisteredLauncherAction { reference, .. } => format!("Run the exact registered primary action {}", reference.action_id),
        RuleAction::MkMacro { macro_id, .. } => format!("Run exact MkMacro ID {macro_id} with its validated defaults"),
    })
}

fn select_action_choice(editor: &mut RuleEditor, choice: ActionChoice) {
    if editor.action_choice == choice {
        return;
    }
    let old = editor.draft.action.clone();
    editor.action_choice = choice;
    editor.picker_open = matches!(choice, ActionChoice::RegisteredAction | ActionChoice::MkMacro);
    editor.draft.action = match (choice, old) {
        (ActionChoice::Copy, RuleAction::Copy { destination })
        | (ActionChoice::Copy, RuleAction::Move { destination }) => RuleAction::Copy { destination },
        (ActionChoice::Move, RuleAction::Move { destination })
        | (ActionChoice::Move, RuleAction::Copy { destination }) => RuleAction::Move { destination },
        (ActionChoice::Copy, _) => RuleAction::Copy { destination: PathBuf::new() },
        (ActionChoice::Move, _) => RuleAction::Move { destination: PathBuf::new() },
        (ActionChoice::Rename, RuleAction::Rename { prefix, suffix }) => RuleAction::Rename { prefix, suffix },
        (ActionChoice::Rename, _) => RuleAction::Rename { prefix: String::new(), suffix: String::new() },
        (ActionChoice::RegisteredAction, action @ RuleAction::RegisteredLauncherAction { .. }) => action,
        (ActionChoice::MkMacro, action @ RuleAction::MkMacro { .. }) => action,
        (ActionChoice::RegisteredAction, _) => RuleAction::Copy { destination: PathBuf::new() },
        (ActionChoice::MkMacro, _) => RuleAction::Copy { destination: PathBuf::new() },
    };
    editor.changed();
}

fn render_action_picker(
    ui: &mut egui::Ui,
    editor: &mut RuleEditor,
    targets: &[ResolvedActionTarget],
    macros: Option<&MkMacroDocument>,
) {
    ui.group(|ui| {
        ui.label("Select an exact current target. Unavailable or interactive actions cannot be enabled.");
        match editor.action_choice {
            ActionChoice::RegisteredAction => {
                if targets.is_empty() {
                    ui.label("Building the current target catalog…");
                }
                egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                    for target in targets {
                        let Ok(action) = crate::hotfolder::bind_registered_launcher_action(target) else {
                            continue;
                        };
                        let target_ref = target.target.persistent_ref();
                        ui.push_id(format!("hotfolder-target-{target_ref:?}"), |ui| {
                            let detail = format!("{} · {:?}", target.selected_action.label, target_ref);
                            if ui.button(detail).clicked() {
                                editor.draft.action = action;
                                editor.picker_open = false;
                                editor.changed();
                            }
                        });
                    }
                });
            }
            ActionChoice::MkMacro => {
                if let Some(macros) = macros {
                    egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                        for macro_ in &macros.macros {
                            let binding = crate::hotfolder::bind_mkmacro_action(macros, macro_.id);
                            let label = format!(
                                "ID {} · {}{}",
                                macro_.id,
                                macro_.name,
                                if macro_.enabled { "" } else { " · disabled" }
                            );
                            let response = ui.add_enabled(
                                binding.is_ok() && macro_.enabled,
                                egui::Button::new(label),
                            );
                            match binding {
                                Ok(action) if response.clicked() => {
                                    editor.draft.action = action;
                                    editor.picker_open = false;
                                    editor.changed();
                                }
                                Err(reason) => {
                                    ui.small(reason);
                                }
                                _ => {}
                            }
                        }
                    });
                } else {
                    ui.label("Building the current MkMacro catalog…");
                }
            }
            _ => {
                ui.label("Choose Registered launcher action or MkMacro to open this exact-target picker.");
            }
        }
        if ui.button("Close Picker").clicked() {
            editor.picker_open = false;
        }
    });
}

fn event_label(event: HotFolderEventKind) -> &'static str {
    match event {
        HotFolderEventKind::Created => "Created",
        HotFolderEventKind::Modified => "Modified",
        HotFolderEventKind::Removed => "Removed",
        HotFolderEventKind::RenamedFrom => "Renamed from",
        HotFolderEventKind::RenamedTo => "Renamed to",
    }
}

fn action_summary(action: &RuleAction) -> String {
    match action {
        RuleAction::Copy { destination } => format!("Copy → {}", destination.display()),
        RuleAction::Move { destination } => format!("Move → {}", destination.display()),
        RuleAction::Rename { prefix, suffix } => format!("Rename with prefix {prefix:?} and suffix {suffix:?}"),
        RuleAction::RegisteredLauncherAction { reference, .. } => format!("Registered action {}", reference.action_id),
        RuleAction::MkMacro { macro_id, .. } => format!("MkMacro ID {macro_id}"),
    }
}

fn rule_scope_summary(rule: &HotFolderRule) -> String {
    let trigger = match rule.trigger {
        RuleTrigger::NewFile => "new file",
        RuleTrigger::ModifiedFile => "modified file",
    };
    let scope = if rule.include_subfolders { "root and subfolders" } else { "direct children" };
    let filter = match (&rule.filter.glob, rule.filter.extensions.is_empty()) {
        (Some(glob), false) => format!("glob {glob:?} and extensions {}", rule.filter.extensions.join(", ")),
        (Some(glob), true) => format!("glob {glob:?}"),
        (None, false) => format!("extensions {}", rule.filter.extensions.join(", ")),
        (None, true) => "all filenames".into(),
    };
    format!("{trigger} · {scope} · {filter} · temporary files {}", if rule.include_temporary_files { "included" } else { "skipped" })
}

fn capability_label(capability: &ActionCapability) -> String {
    match capability {
        ActionCapability::Supported { enablement } => format!("available ({enablement:?})"),
        ActionCapability::Unsupported { reason }
        | ActionCapability::Unavailable { reason }
        | ActionCapability::Invalid { reason } => (*reason).into(),
    }
}

fn action_label(action: HotFolderExecutionAction) -> &'static str {
    match action {
        HotFolderExecutionAction::Copy => "Copy",
        HotFolderExecutionAction::Move => "Move",
        HotFolderExecutionAction::Rename => "Rename",
        HotFolderExecutionAction::RegisteredLauncherAction => "Registered action",
        HotFolderExecutionAction::MkMacro => "MkMacro",
    }
}

fn status_label(status: HotFolderTerminalStatus) -> &'static str {
    match status {
        HotFolderTerminalStatus::Success => "Success",
        HotFolderTerminalStatus::Failure => "Failure",
        HotFolderTerminalStatus::Partial => "Partial",
        HotFolderTerminalStatus::Cancelled => "Cancelled",
    }
}

fn basename(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(unknown file)".into())
}

fn format_time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn format_unix_nanos(nanos: u128) -> String {
    let seconds = u64::try_from(nanos / 1_000_000_000).ok();
    let subsecond = (nanos % 1_000_000_000) as u32;
    seconds
        .and_then(|seconds| {
            std::time::UNIX_EPOCH.checked_add(std::time::Duration::new(seconds, subsecond))
        })
        .map(format_time)
        .unwrap_or_else(|| "(time unavailable)".into())
}

#[cfg(test)]
mod m10e_editor_fixtures {
    use super::*;
    use crate::hotfolder::{
        HotFolderExternalActionOutcome, HotFolderExternalActionRequest,
        HotFolderExecutionPhase, HotFolderTerminalStatus,
    };
    use crate::platform::app_data::AppDataRoot;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    struct CountingExternalAdapter {
        dispatches: AtomicUsize,
    }

    impl crate::hotfolder::HotFolderExternalActionAdapter for CountingExternalAdapter {
        fn supports(&self, _action: &RuleAction) -> bool {
            false
        }

        fn dispatch(
            &self,
            request: HotFolderExternalActionRequest,
            _cancellation: &crate::file_operations::ScanCancellation,
        ) -> HotFolderExternalActionOutcome {
            self.dispatches.fetch_add(1, Ordering::SeqCst);
            HotFolderExternalActionOutcome {
                operation_id: request.operation_id,
                macro_run_id: None,
                status: HotFolderTerminalStatus::Failure,
                phase: HotFolderExecutionPhase::Complete,
                reason: Some("fixture adapter must not be called".into()),
            }
        }
    }

    #[test]
    fn m10e_test_rule_reads_sample_metadata_without_live_runtime_effects() {
        let fixture_parent = std::env::current_dir()
            .expect("resolve repository root")
            .join("target/productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).expect("create disposable fixture parent");
        let temporary = tempfile::Builder::new()
            .prefix("hotfolder_sample_test_")
            .tempdir_in(&fixture_parent)
            .expect("fixture root");
        let fixture_root = temporary.path().join("hotfolder_sample_test");
        let watched = fixture_root.join("incoming");
        let destination = fixture_root.join("outgoing");
        std::fs::create_dir_all(&watched).expect("create disposable watched directory");
        std::fs::create_dir_all(&destination).expect("create disposable destination");
        let sample = watched.join("report.txt");
        std::fs::write(&sample, b"unchanged sample bytes").expect("write disposable sample");

        let mut rule = HotFolderRule::new(
            "live rule draft",
            &watched,
            RuleAction::Copy {
                destination: destination.clone(),
            },
        );
        rule.enabled = false;
        rule.filter.extensions = vec!["txt".into()];
        let data_root = AppDataRoot::from_path(fixture_root.join("app_data"));
        let store = crate::hotfolder::HotFolderRulesStore::new(&data_root);
        let empty = store.load();
        let saved = store
            .save_rules(empty.snapshot.revision, vec![rule.clone()])
            .expect("persist enabled live rule");
        let adapter = Arc::new(CountingExternalAdapter {
            dispatches: AtomicUsize::new(0),
        });
        let mut driver = crate::gui::hotfolder_runtime::HotFolderRuntimeDriver::new(
            data_root,
            adapter.clone(),
        );
        let inputs = HotFolderCapabilityInputs::empty();
        let start = driver
            .start(inputs.clone())
            .expect("submit live driver start");
        let start_deadline = Instant::now() + Duration::from_secs(3);
        let started = loop {
            let snapshot = driver.snapshot();
            if snapshot.acknowledgements.iter().any(|ack| ack.request_id == start) {
                assert!(snapshot.running);
                break snapshot;
            }
            assert!(Instant::now() < start_deadline, "live driver did not start");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(!started.store.as_ref().unwrap().rules[0].enabled);
        let enable = driver
            .prepare_enable(saved.revision, rule.id.clone(), inputs.clone())
            .expect("submit explicit rule enable");
        let enable_deadline = Instant::now() + Duration::from_secs(3);
        let before = loop {
            let snapshot = driver.snapshot();
            if let Some(ack) = snapshot
                .acknowledgements
                .iter()
                .find(|ack| ack.request_id == enable)
            {
                ack.result.as_ref().expect("explicit enable succeeds");
                break snapshot;
            }
            assert!(Instant::now() < enable_deadline, "enabled rule was not acknowledged");
            std::thread::sleep(Duration::from_millis(10));
        };
        let before_store = before.store.clone().expect("live store snapshot");
        assert_eq!(before_store.revision, saved.revision + 1);
        assert!(before_store.rules[0].enabled);
        let before_runtime = before.runtime.clone().expect("live watcher snapshot");
        assert_eq!(
            (before_runtime.active_roots, before_runtime.active_rules),
            (1, 1),
            "Test Rule fixture starts with an actually active live watcher"
        );
        let before_execution = before.execution.clone().expect("live C snapshot");
        let before_raw = std::fs::read(store.path()).expect("read persisted rules bytes");

        let mut dialog = HotFolderDialog::default();
        dialog.open();
        let mut editor = RuleEditor::new(
            99,
            EditMode::Existing,
            before_store.revision,
            before_store.rules[0].clone(),
        );
        editor.sample_path = Some(sample.clone());
        dialog.editor = Some(editor);
        let context = egui::Context::default();
        let mut rendered_actions = Vec::new();
        context.run_ui(egui::RawInput::default(), |ctx| {
            rendered_actions = dialog.ui(ctx, &before);
        }).drop_without_applying_deltas();
        assert!(rendered_actions.is_empty(), "rendering the manager has no implicit runtime command");

        let test_action = sample_test_action(dialog.editor.as_ref().expect("current draft"))
            .expect("sample selection creates the UI Test Rule action");
        let unhandled = dialog.start_sample_test_from_action(
            test_action,
            inputs,
            adapter.clone(),
        );
        assert!(unhandled.is_none(), "Test Rule stays inside the read-only worker");

        let deadline = Instant::now() + Duration::from_secs(2);
        while dialog.test_result.is_none() && Instant::now() < deadline {
            let snapshot = driver.snapshot();
            context.run_ui(egui::RawInput::default(), |ctx| {
                let _ = dialog.ui(ctx, &snapshot);
            }).drop_without_applying_deltas();
            std::thread::yield_now();
        }
        let result = dialog.test_result.expect("sample evaluation completed");
        assert!(result.condition.is_ok());
        assert!(result.plan.as_deref().is_some_and(|plan| {
            plan.contains("outgoing") && plan.contains("report.txt")
        }));
        assert!(matches!(result.capability, ActionCapability::Supported { .. }));
        assert_eq!(adapter.dispatches.load(Ordering::SeqCst), 0);
        assert_eq!(std::fs::read(&sample).expect("sample remains readable"), b"unchanged sample bytes");
        assert!(!destination.join("report.txt").exists());
        let after = driver.snapshot();
        let after_store = after.store.as_ref().expect("live store remains available");
        assert_eq!(after_store.revision, before_store.revision);
        assert_eq!(after_store.rules, before_store.rules);
        assert_eq!(std::fs::read(store.path()).expect("rules remain unchanged"), before_raw);
        assert_eq!(after.runtime.as_ref(), Some(&before_runtime));
        assert_eq!(after.execution.as_ref(), Some(&before_execution));
        assert_eq!(after.execution_progress, Default::default());

        let stop = driver.stop().expect("submit live driver stop");
        let stop_deadline = Instant::now() + Duration::from_secs(3);
        while !driver.snapshot().acknowledgements.iter().any(|ack| ack.request_id == stop) {
            assert!(Instant::now() < stop_deadline, "live driver did not acknowledge stop");
            std::thread::sleep(Duration::from_millis(10));
        }
        driver.shutdown_and_reap();
        driver.join_reaper();
    }
}
