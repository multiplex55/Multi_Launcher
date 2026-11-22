//! Resizable Hot Folder rule manager. This module owns only drafts and view
//! state; every persistent or runtime transition is returned as a typed
//! request to the app-owned driver.

use super::jump_mode::{
    TargetKind,
    ui::{JumpModeScope, register_and_apply_target},
};
use crate::gui::hotfolder_runtime::{
    HotFolderCapabilityInputs, HotFolderDriverSnapshot, HotFolderEnablePreview,
    evaluate_sample_conditions,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::hotfolder::{
    ActionCapability, HotFolderEventKind, HotFolderExecutionAction, HotFolderExecutionPhase,
    HotFolderExecutionReceipt, HotFolderRule, HotFolderTerminalStatus, RuleAction, RuleId,
    RuleTrigger, StrongActionConfirmation,
};
use crate::mkmacro::MkMacroDocument;
use crate::universal_actions::ResolvedActionTarget;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::SystemTime;

const HISTORY_ROWS: usize = 128;

fn reveal_hotfolder_target(
    scope: Option<&JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: TargetKind,
    order: &mut u64,
) {
    register_and_apply_target(scope, ui, response, identity, label, kind, order);
    if response.enabled()
        && response.gained_focus()
        && ui.is_visible()
        && !ui.is_sizing_pass()
        && !ui.ctx().will_discard()
        && !ui.clip_rect().contains_rect(response.rect)
    {
        response.scroll_to_me(None);
    }
}

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
    ChoosePath {
        editor_id: u128,
        draft_revision: u64,
        kind: HotFolderPathKind,
    },
    TestRule {
        editor_id: u128,
        rule: HotFolderRule,
        sample: PathBuf,
        event: HotFolderEventKind,
        draft_revision: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotFolderPathKind {
    WatchedDirectory,
    DestinationDirectory,
    SampleFile,
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

#[derive(Clone)]
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

#[derive(Clone)]
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
    fn new(editor_id: u128, mode: EditMode, expected_revision: u64, draft: HotFolderRule) -> Self {
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
        self.validation_error = self.draft.validate().err().map(str::to_owned).or_else(|| {
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
    jump_epoch: u64,
    editor: Option<RuleEditor>,
    picker_targets: Option<Arc<[ResolvedActionTarget]>>,
    picker_macros: Option<Arc<MkMacroDocument>>,
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
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    popup_viewport: Option<egui::ViewportId>,
    pending_frame: Option<Box<HotFolderFrame>>,
}

impl Default for HotFolderDialog {
    fn default() -> Self {
        Self {
            open: false,
            jump_epoch: 0,
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
            popup_anchors: Vec::new(),
            popup_viewport: None,
            pending_frame: None,
        }
    }
}

struct HotFolderViewSnapshot {
    open: bool,
    editor: Option<RuleEditor>,
    picker_targets: Option<Arc<[ResolvedActionTarget]>>,
    picker_macros: Option<Arc<MkMacroDocument>>,
    pending: Option<PendingRequest>,
    error: Option<String>,
    notice: Option<String>,
    delete_confirmation: Option<(u64, HotFolderRule)>,
    undo_delete: Option<HotFolderRule>,
    clear_history_confirmation: bool,
    show_full_paths: bool,
    test_result: Option<SampleTestResult>,
    next_editor_id: u128,
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    popup_viewport: Option<egui::ViewportId>,
}

struct HotFolderFrame {
    pass: super::utility_host::ViewportPassProvenance,
    session: u64,
    store_revision: Option<u64>,
    view: HotFolderViewSnapshot,
    actions: Vec<HotFolderDialogAction>,
}

impl HotFolderDialog {
    fn retire_stale_sample_job(&mut self) {
        if self.test_job.as_ref().is_some_and(|job| {
            !self.open
                || self.editor.as_ref().is_none_or(|editor| {
                    editor.editor_id != job.editor_id || editor.draft_revision != job.draft_revision
                })
        }) {
            self.retire_sample_test();
        }
    }
    fn capture_view(&self) -> HotFolderViewSnapshot {
        HotFolderViewSnapshot {
            open: self.open,
            editor: self.editor.clone(),
            picker_targets: self.picker_targets.clone(),
            picker_macros: self.picker_macros.clone(),
            pending: self.pending.clone(),
            error: self.error.clone(),
            notice: self.notice.clone(),
            delete_confirmation: self.delete_confirmation.clone(),
            undo_delete: self.undo_delete.clone(),
            clear_history_confirmation: self.clear_history_confirmation,
            show_full_paths: self.show_full_paths,
            test_result: self.test_result.clone(),
            next_editor_id: self.next_editor_id,
            popup_anchors: self.popup_anchors.clone(),
            popup_viewport: self.popup_viewport,
        }
    }
    fn restore_view(&mut self, state: HotFolderViewSnapshot) {
        self.open = state.open;
        self.editor = state.editor;
        self.picker_targets = state.picker_targets;
        self.picker_macros = state.picker_macros;
        self.pending = state.pending;
        self.error = state.error;
        self.notice = state.notice;
        self.delete_confirmation = state.delete_confirmation;
        self.undo_delete = state.undo_delete;
        self.clear_history_confirmation = state.clear_history_confirmation;
        self.show_full_paths = state.show_full_paths;
        self.test_result = state.test_result;
        self.next_editor_id = state.next_editor_id;
        self.popup_anchors = state.popup_anchors;
        self.popup_viewport = state.popup_viewport;
    }
    pub(super) fn owns_inline_escape(&self, snapshot: &HotFolderDriverSnapshot) -> bool {
        self.delete_confirmation.is_some()
            || self.clear_history_confirmation
            || self.editor.as_ref().is_some_and(|editor| editor.picker_open)
            || (self.editor.is_none() && !snapshot.enable_previews.is_empty())
    }

    pub(super) fn owned_popup_layer(
        &self,
        ctx: &egui::Context,
        base: egui::LayerId,
    ) -> Option<egui::LayerId> {
        if self.popup_viewport != Some(ctx.viewport_id()) {
            return None;
        }
        self.popup_anchors.iter().find_map(|(id, layer)| {
            if *layer != base
                || !ctx
                    .read_response(*id)
                    .is_some_and(|response| response.enabled() && response.layer_id == base)
            {
                return None;
            }
            let popup = id.with("popup");
            egui::Popup::is_id_open(ctx, popup)
                .then_some(egui::LayerId::new(egui::Order::Foreground, popup))
        })
    }
    pub(super) fn editor_is_current(&self, editor_id: u128, revision: u64) -> bool {
        self.open
            && self.editor.as_ref().is_some_and(|editor| {
                editor.editor_id == editor_id && editor.draft_revision == revision
            })
    }

    pub(super) fn complete_path_choice(
        &mut self,
        editor_id: u128,
        revision: u64,
        kind: HotFolderPathKind,
        path: Option<PathBuf>,
    ) -> bool {
        let Some(editor) = self.editor.as_mut().filter(|editor| {
            self.open && editor.editor_id == editor_id && editor.draft_revision == revision
        }) else {
            return false;
        };
        let Some(path) = path else {
            return true;
        };
        match kind {
            HotFolderPathKind::WatchedDirectory => editor.draft.watched_root = path,
            HotFolderPathKind::DestinationDirectory => match &mut editor.draft.action {
                RuleAction::Copy { destination } | RuleAction::Move { destination } => {
                    *destination = path
                }
                _ => return false,
            },
            HotFolderPathKind::SampleFile => editor.sample_path = Some(path),
        }
        editor.changed();
        self.test_result = None;
        true
    }
    pub(crate) fn open(&mut self) {
        if !self.open {
            self.jump_epoch = self.jump_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
    }

    pub(crate) fn jump_generation(&self, snapshot: &HotFolderDriverSnapshot) -> u64 {
        let preview = snapshot
            .enable_previews
            .iter()
            .find(|preview| {
                self.editor
                    .as_ref()
                    .is_none_or(|editor| editor.draft.id == preview.rule.id)
            })
            .map(|p| (p.request_id, p.resulting_revision));
        egui::Id::new((
            self.jump_epoch,
            snapshot.store.as_ref().map(|s| s.revision),
            preview,
            self.delete_confirmation
                .as_ref()
                .map(|(revision, rule)| (*revision, rule.id.as_str())),
            self.clear_history_confirmation,
            self.editor
                .as_ref()
                .map(|e| (e.editor_id, e.draft_revision, e.picker_open)),
        ))
        .value()
    }
    #[cfg(test)]
    pub(super) fn test_set_clear_confirmation(&mut self, value: bool) {
        self.clear_history_confirmation = value;
    }
    #[cfg(test)]
    pub(super) fn test_clear_confirmation(&self) -> bool {
        self.clear_history_confirmation
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
        self.picker_targets = Some(targets.into());
        self.picker_macros = Some(Arc::new(macros));
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

    pub(super) fn poll_background(&mut self, snapshot: &HotFolderDriverSnapshot) {
        self.poll_sample_test();
        self.poll_acknowledgement(snapshot);
    }

    pub(super) fn stage_root(
        &mut self,
        ctx: &egui::Context,
        snapshot: &HotFolderDriverSnapshot,
        owner: bool,
        first: bool,
        scope: Option<&JumpModeScope>,
    ) {
        if !self.open {
            return;
        }
        let admitting = owner && first && !ctx.will_discard();
        let before = self.capture_view();
        // Drawing edits a candidate; only the live dialog owns a sample receiver.
        let sample_job = self.test_job.take();
        let actions = self.draw(ctx, snapshot, owner, admitting, scope);
        if admitting && !ctx.will_discard() {
            self.pending_frame = Some(Box::new(HotFolderFrame {
                pass: super::utility_host::ViewportPassProvenance::current(
                    ctx,
                    egui::ViewportId::ROOT,
                ),
                session: self.jump_epoch,
                store_revision: snapshot.store.as_ref().map(|store| store.revision),
                view: self.capture_view(),
                actions,
            }));
        }
        self.restore_view(before);
        self.test_job = sample_job;
    }

    pub(super) fn complete_root(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<(Vec<HotFolderDialogAction>, Option<u64>)> {
        let frame = *self.pending_frame.take()?;
        if !self.open || frame.session != self.jump_epoch || !frame.pass.was_retained(ctx) {
            return None;
        }
        self.restore_view(frame.view);
        self.retire_stale_sample_job();
        Some((frame.actions, frame.store_revision))
    }

    #[cfg(test)]
    pub(super) fn ui(
        &mut self,
        ctx: &egui::Context,
        snapshot: &HotFolderDriverSnapshot,
        owner: bool,
        first: bool,
        scope: Option<&JumpModeScope>,
    ) -> Vec<HotFolderDialogAction> {
        if first {
            self.poll_background(snapshot);
        }
        self.draw(
            ctx,
            snapshot,
            owner,
            owner && first && !ctx.will_discard(),
            scope,
        )
    }

    fn draw(
        &mut self,
        ctx: &egui::Context,
        snapshot: &HotFolderDriverSnapshot,
        owner: bool,
        interactive: bool,
        scope: Option<&JumpModeScope>,
    ) -> Vec<HotFolderDialogAction> {
        let mut actions = Vec::new();
        if interactive && !snapshot.loaded && self.open && self.pending.is_none() {
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
            .resizable(true)
            .default_size([940.0, 660.0])
            .min_width(680.0)
            .min_height(420.0)
            .vscroll(true)
            .show_utility(
                ctx,
                "hotfolder-manager",
                egui::Id::new("Hot Folder Manager"),
                Some(&mut window_open),
                crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                    crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                        egui::Vec2::from([940.0, 660.0]),
                    ),
                    egui::Vec2::ZERO,
                    [None, None],
                )
                .with_minimum(egui::vec2(680.0, 420.0))
                .with_maximum(ctx.content_rect().size()),
                |ui| {
                    let mut order = 0;
                    let child = preview
                        .as_ref()
                        .filter(|_| self.editor.is_none())
                        .map(|p| format!("enable/{}/{}", p.rule.id.as_str(), p.request_id))
                        .or_else(|| {
                            self.delete_confirmation.as_ref().map(|(revision, rule)| {
                                format!("delete/{}/{}", rule.id.as_str(), revision)
                            })
                        })
                        .or_else(|| {
                            self.clear_history_confirmation
                                .then(|| "clear-history".to_owned())
                        });
                    if let Some(child) = child {
                        let child_scope = scope.map(|scope| scope.child_namespace(&child));
                        if let Some(preview) = preview.as_ref().filter(|_| self.editor.is_none()) {
                            self.render_strong_confirmation(
                                ui,
                                preview,
                                &mut actions,
                                child_scope.as_ref(),
                                interactive,
                                &mut order,
                            );
                        } else if let Some((revision, rule)) = self.delete_confirmation.clone() {
                            ui.label(format!("Delete rule ‘{}’?", rule.name));
                            ui.horizontal(|ui| {
                                let confirm = ui.button("Delete Rule");
                                reveal_hotfolder_target(
                                    child_scope.as_ref(),
                                    ui,
                                    &confirm,
                                    "confirm",
                                    "Delete Rule",
                                    TargetKind::Button,
                                    &mut order,
                                );
                                if interactive && confirm.clicked() {
                                    actions.push(HotFolderDialogAction::Delete {
                                        expected_revision: revision,
                                        rule,
                                    });
                                    self.delete_confirmation = None;
                                }
                                let cancel = ui.button("Cancel Delete");
                                reveal_hotfolder_target(
                                    child_scope.as_ref(),
                                    ui,
                                    &cancel,
                                    "cancel",
                                    "Cancel Delete",
                                    TargetKind::Button,
                                    &mut order,
                                );
                                if interactive && cancel.clicked() {
                                    self.delete_confirmation = None;
                                }
                            });
                        } else {
                            ui.label(
                                "Clear terminal history; recovery and dedupe records stay intact.",
                            );
                            ui.horizontal(|ui| {
                                let confirm = ui.button("Clear Terminal History");
                                reveal_hotfolder_target(
                                    child_scope.as_ref(),
                                    ui,
                                    &confirm,
                                    "confirm",
                                    "Clear Terminal History",
                                    TargetKind::Button,
                                    &mut order,
                                );
                                if interactive && confirm.clicked() {
                                    actions.push(HotFolderDialogAction::ClearHistory);
                                    self.clear_history_confirmation = false;
                                }
                                let cancel = ui.button("Keep History");
                                reveal_hotfolder_target(
                                    child_scope.as_ref(),
                                    ui,
                                    &cancel,
                                    "cancel",
                                    "Keep History",
                                    TargetKind::Button,
                                    &mut order,
                                );
                                if interactive && cancel.clicked() {
                                    self.clear_history_confirmation = false;
                                }
                            });
                        }
                        if interactive
                            && !egui::Popup::is_any_open(ctx)
                            && ctx.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            })
                        {
                            if let Some(preview) =
                                preview.as_ref().filter(|_| self.editor.is_none())
                            {
                                actions
                                    .push(HotFolderDialogAction::CancelEnable(preview.request_id));
                            } else if self.delete_confirmation.is_some() {
                                self.delete_confirmation = None;
                            } else {
                                self.clear_history_confirmation = false;
                            }
                        }
                        return;
                    }
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

                    let manager_scope = if self.editor.is_none() { scope } else { None };
                    ui.add_enabled_ui(self.editor.is_none(), |ui| {
                        ui.horizontal(|ui| {
                            if snapshot.plugin_enabled {
                                if snapshot.paused {
                                    let clicked = {
                                        let response = ui.button("Resume");
                                        reveal_hotfolder_target(
                                            manager_scope,
                                            ui,
                                            &response,
                                            "resume",
                                            "Resume",
                                            TargetKind::Button,
                                            &mut order,
                                        );
                                        interactive && response.clicked()
                                    };
                                    if clicked {
                                        actions.push(HotFolderDialogAction::Resume);
                                    }
                                } else {
                                    let response = ui.button("Pause");
                                    reveal_hotfolder_target(
                                        manager_scope,
                                        ui,
                                        &response,
                                        "pause",
                                        "Pause",
                                        TargetKind::Button,
                                        &mut order,
                                    );
                                    if interactive && response.clicked() {
                                        actions.push(HotFolderDialogAction::Pause);
                                    }
                                }
                            }
                            let clicked = {
                                let response = ui.add_enabled(
                                    self.pending.is_none(),
                                    egui::Button::new("New Rule"),
                                );
                                reveal_hotfolder_target(
                                    scope,
                                    ui,
                                    &response,
                                    "new",
                                    "New Rule",
                                    TargetKind::Button,
                                    &mut order,
                                );
                                interactive && response.clicked()
                            };
                            if clicked {
                                self.begin_new(
                                    snapshot.store.as_ref().map_or(0, |store| store.revision),
                                );
                            }
                            if let Some(rule) = self.undo_delete.as_ref()
                                && {
                                    let response = ui.button("Undo Delete");
                                    reveal_hotfolder_target(
                                        manager_scope,
                                        ui,
                                        &response,
                                        "undo",
                                        "Undo Delete",
                                        TargetKind::Button,
                                        &mut order,
                                    );
                                    interactive && response.clicked()
                                }
                            {
                                actions.push(HotFolderDialogAction::UndoDelete(rule.clone()));
                            }
                        })
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
                            egui::CollapsingHeader::new(
                                "Quarantine diagnostics (raw rows preserved)",
                            )
                            .show(ui, |ui| {
                                for row in &store.quarantined_rows {
                                    ui.label(row.diagnostic.as_str());
                                }
                            });
                        }
                        ui.separator();
                        if self.editor.is_some() {
                            ui.add_enabled_ui(owner, |ui| {
                                self.render_editor(
                                    ui,
                                    store.revision,
                                    &mut actions,
                                    scope,
                                    interactive,
                                    &mut order,
                                )
                            });
                        } else {
                            self.render_rules(
                                ui,
                                snapshot,
                                &mut actions,
                                scope,
                                interactive,
                                &mut order,
                            );
                        }
                    } else {
                        ui.label("Loading stored rules…");
                    }

                    ui.separator();
                    self.render_runtime_status(ui, snapshot);
                    ui.separator();
                    ui.add_enabled_ui(self.editor.is_none(), |ui| {
                        self.render_history(ui, snapshot, manager_scope, interactive, &mut order)
                    });
                },
            );
        if interactive {
            self.open = window_open;
        }

        let cancellation_pending = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.kind == PendingKind::CancelEnable);
        if interactive
            && (was_open && !self.open || self.editor.is_some())
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
                        self.notice =
                            Some("Rule saved disabled. Enable it explicitly after review.".into());
                    }
                    PendingKind::Delete => {
                        self.undo_delete = pending.undo_rule;
                    }
                    PendingKind::UndoDelete => {
                        self.undo_delete = None;
                    }
                    PendingKind::CancelEnable => {}
                    PendingKind::ClearHistory => {
                        self.notice = Some(
                            "Terminal history cleared; dedupe and recovery records were retained."
                                .into(),
                        );
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
        self.editor = Some(RuleEditor::new(
            self.allocate_editor_id(),
            EditMode::New,
            revision,
            rule,
        ));
        self.picker_targets = None;
        self.picker_macros = None;
        self.retire_sample_test();
        self.error = None;
    }

    pub(super) fn open_inbox_rule(&mut self, revision: u64, rule: HotFolderRule) -> bool {
        if let Some(editor) = &self.editor {
            return editor.draft.id == rule.id;
        }
        if self.pending.is_some() {
            return false;
        }
        self.begin_edit(revision, rule);
        true
    }

    fn begin_edit(&mut self, revision: u64, rule: HotFolderRule) {
        self.editor = Some(RuleEditor::new(
            self.allocate_editor_id(),
            EditMode::Existing,
            revision,
            rule,
        ));
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
        scope: Option<&JumpModeScope>,
        interactive: bool,
        order: &mut u64,
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
                    ui.push_id(("hotfolder-rule", rule.id.as_str()), |ui| {
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
                                let clicked = {
                                    let response = ui.add_enabled(
                                        self.pending.is_none(),
                                        egui::Button::new("Edit"),
                                    );
                                    reveal_hotfolder_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("rule/{}/edit", rule.id.as_str()),
                                        format!("Edit {}", rule.name),
                                        TargetKind::Button,
                                        order,
                                    );
                                    interactive && response.clicked()
                                };
                                if clicked {
                                    self.begin_edit(store.revision, rule.clone());
                                }
                                if snapshot.plugin_enabled && rule.enabled {
                                    let clicked = {
                                        let response = ui.button("Disable");
                                        reveal_hotfolder_target(
                                            scope,
                                            ui,
                                            &response,
                                            format!("rule/{}/disable", rule.id.as_str()),
                                            format!("Disable {}", rule.name),
                                            TargetKind::Button,
                                            order,
                                        );
                                        interactive && response.clicked()
                                    };
                                    if clicked {
                                        actions.push(HotFolderDialogAction::Disable {
                                            expected_revision: store.revision,
                                            rule_id: rule.id.clone(),
                                        });
                                    }
                                } else if snapshot.plugin_enabled
                                    && !rule.enabled
                                    && !snapshot.paused
                                    && snapshot.enable_previews.is_empty()
                                {
                                    let response = ui.button("Enable…");
                                    reveal_hotfolder_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("rule/{}/enable", rule.id.as_str()),
                                        format!("Enable {}", rule.name),
                                        TargetKind::Button,
                                        order,
                                    );
                                    if interactive && response.clicked() {
                                        actions.push(HotFolderDialogAction::PrepareEnable {
                                            expected_revision: store.revision,
                                            rule_id: rule.id.clone(),
                                        });
                                    }
                                }
                                let clicked = {
                                    let response = ui.button("Delete…");
                                    reveal_hotfolder_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("rule/{}/delete", rule.id.as_str()),
                                        format!("Delete {}", rule.name),
                                        TargetKind::Button,
                                        order,
                                    );
                                    interactive && response.clicked()
                                };
                                if clicked {
                                    self.delete_confirmation = Some((store.revision, rule.clone()));
                                }
                            });
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
        scope: Option<&JumpModeScope>,
        interactive: bool,
        order: &mut u64,
    ) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let editor_scope =
            scope.map(|scope| scope.child_namespace(format!("editor/{}", editor.editor_id)));
        let scope = editor_scope.as_ref();
        if editor.picker_open {
            let child = scope.map(|scope| scope.child_namespace("action-picker"));
            render_action_picker(
                ui,
                &mut editor,
                self.picker_targets.as_deref().unwrap_or_default(),
                self.picker_macros.as_deref(),
                child.as_ref(),
                order,
            );
            let cancel = ui.button("Close target picker");
            reveal_hotfolder_target(
                child.as_ref(),
                ui,
                &cancel,
                "close",
                "Close target picker",
                TargetKind::Button,
                order,
            );
            if interactive
                && (cancel.clicked()
                    || ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    }))
            {
                editor.picker_open = false;
            }
            if !editor.picker_open {
                self.picker_targets = None;
                self.picker_macros = None;
            }
            self.editor = Some(editor);
            return;
        }
        self.popup_anchors.clear();
        self.popup_viewport = Some(ui.ctx().viewport_id());
        macro_rules! target {
            ($ui:expr, $response:expr, $id:expr, $label:expr, $kind:expr) => {{
                let response = $response;
                reveal_hotfolder_target(scope, $ui, &response, $id, $label, $kind, order);
                response
            }};
        }
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
                target!(
                    ui,
                    ui.text_edit_singleline(&mut editor.draft.name),
                    "name",
                    "Rule name",
                    TargetKind::TextEntry
                );
                ui.end_row();

                ui.label("Watched directory");
                ui.horizontal(|ui| {
                    ui.label(editor.draft.watched_root.display().to_string());
                    if target!(
                        ui,
                        ui.button("Choose directory…"),
                        "watched",
                        "Choose watched directory",
                        TargetKind::Button
                    )
                    .clicked()
                    {
                        requested_picker = Some(0_u8);
                    }
                });
                ui.end_row();

                ui.label("Trigger");
                let popup_scope = scope.map(|scope| scope.combo_popup_scope("trigger"));
                let trigger = egui::ComboBox::from_id_salt("hotfolder_trigger")
                    .selected_text(match editor.draft.trigger {
                        RuleTrigger::NewFile => "New file",
                        RuleTrigger::ModifiedFile => "Modified file",
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (RuleTrigger::NewFile, "New file"),
                            (RuleTrigger::ModifiedFile, "Modified file"),
                        ] {
                            let response = ui
                                .push_id(label, |ui| {
                                    ui.selectable_value(&mut editor.draft.trigger, value, label)
                                })
                                .inner;
                            reveal_hotfolder_target(
                                popup_scope.as_ref(),
                                ui,
                                &response,
                                format!("option/{label}"),
                                label,
                                TargetKind::Selectable,
                                order,
                            );
                        }
                    })
                    .response;
                reveal_hotfolder_target(
                    scope,
                    ui,
                    &trigger,
                    "trigger",
                    "Trigger",
                    TargetKind::ComboBox,
                    order,
                );
                self.popup_anchors.push((trigger.id, trigger.layer_id));
                ui.end_row();

                ui.label("Filename glob (* and ?)");
                target!(
                    ui,
                    ui.text_edit_singleline(&mut editor.glob_text),
                    "glob",
                    "Filename glob",
                    TargetKind::TextEntry
                );
                ui.end_row();

                ui.label("Extensions (comma separated)");
                target!(
                    ui,
                    ui.text_edit_singleline(&mut editor.extensions_text),
                    "extensions",
                    "Extensions",
                    TargetKind::TextEntry
                );
                ui.end_row();

                ui.label("Scope");
                ui.vertical(|ui| {
                    target!(
                        ui,
                        ui.checkbox(&mut editor.draft.include_subfolders, "Include subfolders"),
                        "subfolders",
                        "Include subfolders",
                        TargetKind::Checkbox
                    );
                    target!(
                        ui,
                        ui.checkbox(
                            &mut editor.draft.include_temporary_files,
                            "Include .part and .crdownload files"
                        ),
                        "temporary",
                        "Include temporary files",
                        TargetKind::Checkbox
                    );
                });
                ui.end_row();

                ui.label("Settle policy");
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::DragValue::new(&mut editor.draft.settle.sample_interval_ms)
                            .range(50..=5000)
                            .suffix(" ms interval"),
                    );
                    reveal_hotfolder_target(
                        scope,
                        ui,
                        &response,
                        "settle-interval",
                        "Sample interval",
                        TargetKind::TextEntry,
                        order,
                    );
                    let response = ui.add(
                        egui::DragValue::new(&mut editor.draft.settle.stable_observations)
                            .range(2..=10)
                            .suffix(" stable observations"),
                    );
                    reveal_hotfolder_target(
                        scope,
                        ui,
                        &response,
                        "settle-observations",
                        "Stable observations",
                        TargetKind::TextEntry,
                        order,
                    );
                    let response = ui.add(
                        egui::DragValue::new(&mut editor.draft.settle.timeout_ms)
                            .range(100..=30000)
                            .suffix(" ms timeout"),
                    );
                    reveal_hotfolder_target(
                        scope,
                        ui,
                        &response,
                        "settle-timeout",
                        "Settle timeout",
                        TargetKind::TextEntry,
                        order,
                    );
                });
                ui.end_row();

                ui.label("Action");
                ui.horizontal_wrapped(|ui| {
                    for choice in [
                        ActionChoice::Copy,
                        ActionChoice::Move,
                        ActionChoice::Rename,
                        ActionChoice::RegisteredAction,
                        ActionChoice::MkMacro,
                    ] {
                        if target!(
                            ui,
                            ui.selectable_label(editor.action_choice == choice, choice.label()),
                            format!("action/{}", choice.label()),
                            choice.label(),
                            TargetKind::Selectable
                        )
                        .clicked()
                        {
                            select_action_choice(&mut editor, choice);
                        }
                    }
                });
                ui.end_row();

                match (&mut editor.draft.action, editor.action_choice) {
                    (
                        RuleAction::Copy { destination } | RuleAction::Move { destination },
                        ActionChoice::Copy | ActionChoice::Move,
                    ) => {
                        ui.label("Destination directory");
                        ui.horizontal(|ui| {
                            ui.label(destination.display().to_string());
                            if target!(
                                ui,
                                ui.button("Choose directory…"),
                                "destination",
                                "Choose destination directory",
                                TargetKind::Button
                            )
                            .clicked()
                            {
                                requested_picker = Some(1);
                            }
                        });
                        ui.end_row();
                    }
                    (RuleAction::Rename { prefix, suffix }, ActionChoice::Rename) => {
                        ui.label("Rename prefix");
                        target!(
                            ui,
                            ui.text_edit_singleline(prefix),
                            "rename-prefix",
                            "Rename prefix",
                            TargetKind::TextEntry
                        );
                        ui.end_row();
                        ui.label("Rename suffix");
                        target!(
                            ui,
                            ui.text_edit_singleline(suffix),
                            "rename-suffix",
                            "Rename suffix",
                            TargetKind::TextEntry
                        );
                        ui.end_row();
                    }
                    (
                        RuleAction::RegisteredLauncherAction { .. },
                        ActionChoice::RegisteredAction,
                    )
                    | (RuleAction::MkMacro { .. }, ActionChoice::MkMacro) => {
                        ui.label("Exact action");
                        ui.horizontal(|ui| {
                            ui.label(action_summary(&editor.draft.action));
                            if target!(
                                ui,
                                ui.button("Choose exact current target…"),
                                "choose-target",
                                "Choose exact target",
                                TargetKind::Button
                            )
                            .clicked()
                            {
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
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "Choose a current registered action or macro.",
                        );
                        ui.end_row();
                    }
                }
            });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Sample file (read-only metadata only)");
            if target!(
                ui,
                ui.button("Choose sample file…"),
                "sample",
                "Choose sample file",
                TargetKind::Button
            )
            .clicked()
            {
                requested_picker = Some(2);
            }
            ui.label(editor.sample_path.as_ref().map_or_else(
                || "No sample selected".into(),
                |path| path.display().to_string(),
            ));
        });
        let popup_scope = scope.map(|scope| scope.combo_popup_scope("sample-event"));
        let sample_event = egui::ComboBox::from_id_salt("hotfolder_test_event")
            .selected_text(event_label(editor.sample_event))
            .show_ui(ui, |ui| {
                for value in [
                    HotFolderEventKind::Created,
                    HotFolderEventKind::Modified,
                    HotFolderEventKind::RenamedTo,
                ] {
                    let label = event_label(value);
                    let response = ui
                        .push_id(label, |ui| {
                            ui.selectable_value(&mut editor.sample_event, value, label)
                        })
                        .inner;
                    reveal_hotfolder_target(
                        popup_scope.as_ref(),
                        ui,
                        &response,
                        format!("option/{label}"),
                        label,
                        TargetKind::Selectable,
                        order,
                    );
                }
            })
            .response;
        reveal_hotfolder_target(
            scope,
            ui,
            &sample_event,
            "sample-event",
            "Sample event",
            TargetKind::ComboBox,
            order,
        );
        self.popup_anchors
            .push((sample_event.id, sample_event.layer_id));

        editor.validate();
        if let Some(error) = editor.validation_error.as_deref() {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Validation: {error}"));
        } else {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                "Draft is structurally valid; current capability is checked again on Enable.",
            );
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
            if target!(
                ui,
                ui.add_enabled(
                    !saving
                        && editor.validation_error.is_none()
                        && editor.expected_revision == current_revision,
                    egui::Button::new("Save Disabled Rule"),
                ),
                "save",
                "Save Disabled Rule",
                TargetKind::Button
            )
            .clicked()
            {
                let mut rule = editor.draft.clone();
                rule.enabled = false;
                actions.push(match editor.mode {
                    EditMode::New => HotFolderDialogAction::SaveNew {
                        expected_revision: editor.expected_revision,
                        rule,
                    },
                    EditMode::Existing => HotFolderDialogAction::SaveEdit {
                        expected_revision: editor.expected_revision,
                        rule,
                    },
                });
            }
            if target!(
                ui,
                ui.add_enabled(
                    !saving && editor.sample_path.is_some(),
                    egui::Button::new("Test Rule (zero effects)"),
                ),
                "test",
                "Test Rule (zero effects)",
                TargetKind::Button
            )
            .clicked()
                && let Some(action) = sample_test_action(&editor)
            {
                actions.push(action);
            }
            let cancel = target!(
                ui,
                ui.add_enabled(!saving, egui::Button::new("Cancel Draft")),
                "cancel",
                "Cancel Draft",
                TargetKind::Button
            );
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("test_hotfolder_editor_cancel"),
                    (cancel.id, cancel.rect, ui.clip_rect()),
                )
            });
            if cancel.clicked() {
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
        if interactive && let Some(field) = requested_picker {
            let kind = match field {
                0 => HotFolderPathKind::WatchedDirectory,
                1 => HotFolderPathKind::DestinationDirectory,
                _ => HotFolderPathKind::SampleFile,
            };
            actions.push(HotFolderDialogAction::ChoosePath {
                editor_id: editor.editor_id,
                draft_revision: editor.draft_revision,
                kind,
            });
        }
        if !cancel_draft {
            self.editor = Some(editor);
        } else {
            self.editor = None;
        }
    }

    fn render_test_result(&self, ui: &mut egui::Ui, result: &SampleTestResult) {
        ui.group(|ui| {
            ui.strong("Pure sample evaluation");
            ui.label(format!(
                "Sample: {}",
                result
                    .sample
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
            if let Some(size) = result.size {
                ui.label(format!(
                    "Read-only metadata: {size} bytes{}",
                    result
                        .modified
                        .map_or_else(String::new, |time| format!(" · {}", format_time(time)))
                ));
            }
            match &result.condition {
                Ok(()) => {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Conditions: match");
                    if let Some(plan) = result.plan.as_deref() {
                        ui.label(format!("Planned action: {plan}"));
                    }
                }
                Err(reason) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Conditions: no match — {reason}"),
                    );
                }
            }
            ui.label(format!(
                "Capability: {}",
                capability_label(&result.capability)
            ));
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
                ui.small(format!(
                    "Watcher diagnostic: {:?}{}",
                    diagnostic.kind,
                    diagnostic
                        .rule_id
                        .as_ref()
                        .map_or_else(String::new, |id| format!(" · rule {}", id.as_str()))
                ));
            }
        }
        if let Some(execution) = snapshot.execution.as_ref()
            && let Some(error) = execution.journal_error.as_deref()
        {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Execution journal: {error}"),
            );
        }
    }

    fn render_history(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &HotFolderDriverSnapshot,
        scope: Option<&JumpModeScope>,
        interactive: bool,
        order: &mut u64,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Terminal history");
            let mut show = self.show_full_paths;
            let response = ui.checkbox(&mut show, "Show full paths");
            reveal_hotfolder_target(
                scope,
                ui,
                &response,
                "history/paths",
                "Show full paths",
                TargetKind::Checkbox,
                order,
            );
            if interactive {
                self.show_full_paths = show;
            }
            let clear = ui.add_enabled(
                snapshot.execution.is_some(),
                egui::Button::new("Clear History…"),
            );
            reveal_hotfolder_target(
                scope,
                ui,
                &clear,
                "history/clear",
                "Clear History",
                TargetKind::Button,
                order,
            );
            if interactive && clear.clicked() {
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
        scope: Option<&JumpModeScope>,
        interactive: bool,
        order: &mut u64,
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
                let confirm = ui.add_enabled(self.pending.is_none(), egui::Button::new("Confirm Enable"));
                reveal_hotfolder_target(scope, ui, &confirm, "confirm", "Confirm Enable", TargetKind::Button, order);
                if interactive && confirm.clicked() {
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
                let cancel = ui.add_enabled(self.pending.is_none(), egui::Button::new("Cancel Enable"));
                reveal_hotfolder_target(scope, ui, &cancel, "cancel", "Cancel Enable", TargetKind::Button, order);
                if interactive && cancel.clicked() {
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
            let result = evaluate_sample_test(
                editor_id,
                rule,
                sample,
                event,
                draft_revision,
                &inputs,
                adapter.as_ref(),
            );
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
    let metadata = sample_path
        .as_ref()
        .ok()
        .and_then(|path| std::fs::metadata(path).ok());
    let relative = root.as_ref().ok().and_then(|root| {
        sample_path
            .as_ref()
            .ok()
            .and_then(|path| crate::hotfolder::relative_path_beneath(root, path))
    });
    let condition = match (&root, &sample_path, &metadata, &relative) {
        (Err(_), _, _, _) => Err("watched root could not be opened for read-only metadata".into()),
        (_, Err(_), _, _) => Err("sample file could not be opened for read-only metadata".into()),
        (_, _, None, _) => Err("sample metadata is unavailable".into()),
        (_, _, Some(metadata), _) if !metadata.is_file() => Err("sample path is not a file".into()),
        (_, _, _, None) => Err("sample file is outside the watched root".into()),
        (_, _, _, Some(relative)) => {
            evaluate_sample_conditions(&rule, event, relative).map_err(str::to_owned)
        }
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
        RuleAction::Copy { destination } => {
            format!("Copy to {}", destination.join(filename).display())
        }
        RuleAction::Move { destination } => {
            format!("Move to {}", destination.join(filename).display())
        }
        RuleAction::Rename { prefix, suffix } => {
            match crate::batch_rename::transform_filename_with_prefix_suffix(
                filename, prefix, suffix,
            ) {
                Ok(next) => format!("Rename to {}", sample.with_file_name(next).display()),
                Err(_) => "Rename plan is invalid".into(),
            }
        }
        RuleAction::RegisteredLauncherAction { reference, .. } => format!(
            "Run the exact registered primary action {}",
            reference.action_id
        ),
        RuleAction::MkMacro { macro_id, .. } => {
            format!("Run exact MkMacro ID {macro_id} with its validated defaults")
        }
    })
}

fn select_action_choice(editor: &mut RuleEditor, choice: ActionChoice) {
    if editor.action_choice == choice {
        return;
    }
    let old = editor.draft.action.clone();
    editor.action_choice = choice;
    editor.picker_open = matches!(
        choice,
        ActionChoice::RegisteredAction | ActionChoice::MkMacro
    );
    editor.draft.action = match (choice, old) {
        (ActionChoice::Copy, RuleAction::Copy { destination })
        | (ActionChoice::Copy, RuleAction::Move { destination }) => {
            RuleAction::Copy { destination }
        }
        (ActionChoice::Move, RuleAction::Move { destination })
        | (ActionChoice::Move, RuleAction::Copy { destination }) => {
            RuleAction::Move { destination }
        }
        (ActionChoice::Copy, _) => RuleAction::Copy {
            destination: PathBuf::new(),
        },
        (ActionChoice::Move, _) => RuleAction::Move {
            destination: PathBuf::new(),
        },
        (ActionChoice::Rename, RuleAction::Rename { prefix, suffix }) => {
            RuleAction::Rename { prefix, suffix }
        }
        (ActionChoice::Rename, _) => RuleAction::Rename {
            prefix: String::new(),
            suffix: String::new(),
        },
        (ActionChoice::RegisteredAction, action @ RuleAction::RegisteredLauncherAction { .. }) => {
            action
        }
        (ActionChoice::MkMacro, action @ RuleAction::MkMacro { .. }) => action,
        (ActionChoice::RegisteredAction, _) => RuleAction::Copy {
            destination: PathBuf::new(),
        },
        (ActionChoice::MkMacro, _) => RuleAction::Copy {
            destination: PathBuf::new(),
        },
    };
    editor.changed();
}

fn render_action_picker(
    ui: &mut egui::Ui,
    editor: &mut RuleEditor,
    targets: &[ResolvedActionTarget],
    macros: Option<&MkMacroDocument>,
    scope: Option<&JumpModeScope>,
    order: &mut u64,
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
                            let response = ui.button(detail);
                            reveal_hotfolder_target(scope, ui, &response, format!("launcher/{target_ref:?}"), "Select launcher action", TargetKind::Button, order);
                            if response.clicked() {
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
                            ui.push_id(("macro", macro_.id), |ui| {
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
                            reveal_hotfolder_target(scope, ui, &response, format!("macro/{}", macro_.id), format!("Select macro {}", macro_.name), TargetKind::Button, order);
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
                            });
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
        RuleAction::Rename { prefix, suffix } => {
            format!("Rename with prefix {prefix:?} and suffix {suffix:?}")
        }
        RuleAction::RegisteredLauncherAction { reference, .. } => {
            format!("Registered action {}", reference.action_id)
        }
        RuleAction::MkMacro { macro_id, .. } => format!("MkMacro ID {macro_id}"),
    }
}

fn rule_scope_summary(rule: &HotFolderRule) -> String {
    let trigger = match rule.trigger {
        RuleTrigger::NewFile => "new file",
        RuleTrigger::ModifiedFile => "modified file",
    };
    let scope = if rule.include_subfolders {
        "root and subfolders"
    } else {
        "direct children"
    };
    let filter = match (&rule.filter.glob, rule.filter.extensions.is_empty()) {
        (Some(glob), false) => format!(
            "glob {glob:?} and extensions {}",
            rule.filter.extensions.join(", ")
        ),
        (Some(glob), true) => format!("glob {glob:?}"),
        (None, false) => format!("extensions {}", rule.filter.extensions.join(", ")),
        (None, true) => "all filenames".into(),
    };
    format!(
        "{trigger} · {scope} · {filter} · temporary files {}",
        if rule.include_temporary_files {
            "included"
        } else {
            "skipped"
        }
    )
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
        HotFolderExecutionPhase, HotFolderExternalActionOutcome, HotFolderExternalActionRequest,
        HotFolderTerminalStatus,
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
    fn hotfolder_tab_reveals_the_clipped_editor_footer() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| {
            style.animation_time = 0.0;
            style.scroll_animation = egui::style::ScrollAnimation::none();
        });
        let mut dialog = HotFolderDialog::default();
        dialog.open();
        dialog.begin_new(0);
        let snapshot = HotFolderDriverSnapshot {
            loaded: true,
            store: Some(crate::hotfolder::HotFolderRulesSnapshot::for_preview(
                dialog.editor.as_ref().unwrap().draft.clone(),
            )),
            ..Default::default()
        };
        let frame = |dialog: &mut HotFolderDialog, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(720.0, 410.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let actions = dialog.ui(ui.ctx(), &snapshot, true, true, None);
                    assert!(actions.is_empty());
                },
            )
            .drop_without_applying_deltas();
            ctx.data(|data| {
                data.get_temp::<(egui::Id, egui::Rect, egui::Rect)>(egui::Id::new(
                    "test_hotfolder_editor_cancel",
                ))
                .unwrap()
            })
        };
        let mut initial = frame(&mut dialog, Vec::new());
        for _ in 0..3 {
            initial = frame(&mut dialog, Vec::new());
        }
        let (cancel, rect, clip) = initial;
        assert!(!clip.contains_rect(rect), "the footer starts off screen");
        let key = |pressed| egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        for _ in 0..48 {
            frame(&mut dialog, vec![key(true)]);
            frame(&mut dialog, vec![key(false)]);
            if ctx.memory(|memory| memory.focused()) == Some(cancel) {
                break;
            }
        }
        let (_, rect, clip) = frame(&mut dialog, Vec::new());
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(cancel));
        assert!(clip.contains_rect(rect), "the focused footer is revealed");
        assert!(dialog.editor.is_some(), "focus does not dismiss the draft");
    }

    #[test]
    fn hotfolder_editor_popup_and_path_ticket_are_bound() {
        let ctx = egui::Context::default();
        let mut dialog = HotFolderDialog::default();
        dialog.open();
        dialog.begin_new(0);
        let (id, revision) = {
            let editor = dialog.editor.as_ref().unwrap();
            (editor.editor_id, editor.draft_revision)
        };
        let mut actions = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            dialog.render_editor(ui, 0, &mut actions, None, true, &mut 0);
        })
        .drop_without_applying_deltas();
        let (anchor, base) = dialog.popup_anchors[0];
        egui::Popup::open_id(&ctx, anchor.with("popup"));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            dialog.render_editor(ui, 0, &mut actions, None, true, &mut 0);
        })
        .drop_without_applying_deltas();
        assert!(dialog.owned_popup_layer(&ctx, base).is_some());
        assert!(dialog.complete_path_choice(
            id,
            revision,
            HotFolderPathKind::WatchedDirectory,
            Some(PathBuf::from(r"C:\watched"))
        ));
        assert!(!dialog.complete_path_choice(
            id,
            revision,
            HotFolderPathKind::SampleFile,
            Some(PathBuf::from(r"C:\old.txt"))
        ));
        assert!(actions.is_empty());
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
        let mut driver =
            crate::gui::hotfolder_runtime::HotFolderRuntimeDriver::new(data_root, adapter.clone());
        let inputs = HotFolderCapabilityInputs::empty();
        let start = driver
            .start(inputs.clone())
            .expect("submit live driver start");
        let start_deadline = Instant::now() + Duration::from_secs(3);
        let started = loop {
            let snapshot = driver.snapshot();
            if snapshot
                .acknowledgements
                .iter()
                .any(|ack| ack.request_id == start)
            {
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
            assert!(
                Instant::now() < enable_deadline,
                "enabled rule was not acknowledged"
            );
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
        context
            .run_ui(egui::RawInput::default(), |ctx| {
                rendered_actions = dialog.ui(ctx, &before, true, true, None);
            })
            .drop_without_applying_deltas();
        assert!(
            rendered_actions.is_empty(),
            "rendering the manager has no implicit runtime command"
        );

        let test_action = sample_test_action(dialog.editor.as_ref().expect("current draft"))
            .expect("sample selection creates the UI Test Rule action");
        let unhandled = dialog.start_sample_test_from_action(test_action, inputs, adapter.clone());
        assert!(
            unhandled.is_none(),
            "Test Rule stays inside the read-only worker"
        );

        let deadline = Instant::now() + Duration::from_secs(2);
        while dialog.test_result.is_none() && Instant::now() < deadline {
            let snapshot = driver.snapshot();
            context
                .run_ui(egui::RawInput::default(), |ctx| {
                    let _ = dialog.ui(ctx, &snapshot, true, true, None);
                })
                .drop_without_applying_deltas();
            std::thread::yield_now();
        }
        let result = dialog.test_result.expect("sample evaluation completed");
        assert!(result.condition.is_ok());
        assert!(
            result
                .plan
                .as_deref()
                .is_some_and(|plan| { plan.contains("outgoing") && plan.contains("report.txt") })
        );
        assert!(matches!(
            result.capability,
            ActionCapability::Supported { .. }
        ));
        assert_eq!(adapter.dispatches.load(Ordering::SeqCst), 0);
        assert_eq!(
            std::fs::read(&sample).expect("sample remains readable"),
            b"unchanged sample bytes"
        );
        assert!(!destination.join("report.txt").exists());
        let after = driver.snapshot();
        let after_store = after.store.as_ref().expect("live store remains available");
        assert_eq!(after_store.revision, before_store.revision);
        assert_eq!(after_store.rules, before_store.rules);
        assert_eq!(
            std::fs::read(store.path()).expect("rules remain unchanged"),
            before_raw
        );
        assert_eq!(after.runtime.as_ref(), Some(&before_runtime));
        assert_eq!(after.execution.as_ref(), Some(&before_execution));
        assert_eq!(after.execution_progress, Default::default());

        let stop = driver.stop().expect("submit live driver stop");
        let stop_deadline = Instant::now() + Duration::from_secs(3);
        while !driver
            .snapshot()
            .acknowledgements
            .iter()
            .any(|ack| ack.request_id == stop)
        {
            assert!(
                Instant::now() < stop_deadline,
                "live driver did not acknowledge stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        driver.shutdown_and_reap();
        let _ = driver.join_reaper();
    }
}
