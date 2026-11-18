//! Retained, read-only transfer view. Native and worker actions leave as intents.
use crate::file_transfer::{
    CopyVerification, EntryKind, ExecutionState, GroupChoice, ReceiptDetail, ReceiptEntryStatus,
    ReceiptEvidenceRole, ReceiptHealth, ReceiptObservation, ReceiptSummary, ReceiptTarget,
    ReviewChoice, ReviewGeneration, ReviewState, SourceNodeId, SourceObservation, TargetOccupancy,
    TransferAction, TransferDraft, TransferHistoryQuery, TransferJobId, TransferJobResult,
    TransferJobStatus, TransferLeaf, TransferQueueSnapshot, TransferReceiptId, TransferReview,
};
use crate::gui::{
    jump_mode::{
        TargetKind,
        ui::{
            JumpModeScope, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
            register_and_apply_target,
        },
    },
    utility_host::{UtilityPresentation, presentation_button},
    utility_window_host::HostedUtilityDialog,
};
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::PathBuf,
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionKind {
    Copy,
    Move,
    Duplicate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PickerKind {
    Files,
    Folder,
    Destination,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DraftTicket {
    pub session: u64,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReviewTicket {
    pub draft: DraftTicket,
    pub generation: ReviewGeneration,
}

#[derive(Debug)]
pub(super) enum Command {
    Closed,
    Pick {
        ticket: DraftTicket,
        kind: PickerKind,
        hwnd: usize,
    },
    Review {
        ticket: DraftTicket,
        sources: Vec<PathBuf>,
        draft: TransferDraft,
    },
    Choice {
        ticket: ReviewTicket,
        node: SourceNodeId,
        choice: ReviewChoice,
    },
    Group {
        ticket: ReviewTicket,
        nodes: Vec<SourceNodeId>,
        choice: GroupChoice,
    },
    Queue {
        ticket: ReviewTicket,
    },
    Start,
    Reorder {
        job: TransferJobId,
        before: Option<TransferJobId>,
    },
    Pause(TransferJobId),
    Resume(TransferJobId),
    Cancel(TransferJobId),
    History(TransferHistoryQuery),
    ConfirmRecovery {
        token: u64,
        verb: RecoveryVerb,
    },
    CancelRecovery {
        token: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecoveryVerb {
    Retry,
    Undo,
}
#[derive(Clone, Debug)]
pub(super) struct RecoveryPreview {
    pub token: u64,
    pub verb: RecoveryVerb,
    pub source: String,
    pub destination: String,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub queue: TransferQueueSnapshot,
    pub history: std::sync::Arc<[ReceiptSummary]>,
    pub detail: Option<std::sync::Arc<ReceiptDetail>>,
    pub recovery: Option<RecoveryPreview>,
    pub busy: bool,
    pub history_waiting: bool,
}
#[derive(Clone, Default)]
struct Edit {
    action: ActionKind,
    verify: bool,
    selected: HashSet<SourceNodeId>,
    alternatives: HashMap<SourceNodeId, String>,
    review_page: usize,
    detail_page: usize,
    terminal_page: Option<(TransferJobId, usize)>,
}
impl Default for ActionKind {
    fn default() -> Self {
        Self::Copy
    }
}
#[derive(Default)]
pub(super) struct FileOperationsCenterDialog {
    pub open: bool,
    session: u64,
    revision: u64,
    sources: Vec<PathBuf>,
    destination: Option<PathBuf>,
    edit: Edit,
    review: Option<Arc<TransferReview>>,
    focus_job: Option<TransferJobId>,
    notice: Option<String>,
}
impl FileOperationsCenterDialog {
    pub fn open(&mut self) {
        if !self.open {
            if let Some(next) = self.session.checked_add(1) {
                self.session = next;
                self.open = true;
            }
        }
    }
    pub fn focus_job(&mut self, job: Option<TransferJobId>) {
        self.focus_job = job;
    }

    pub fn ensure_open(&mut self) {
        self.open();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.review = None;
        self.edit.selected.clear();
        self.edit.alternatives.clear();
    }
    pub fn open_with_sources(&mut self, sources: Vec<PathBuf>) {
        self.open();
        if self.open {
            self.sources = sources;
            self.changed();
        }
    }
    fn changed(&mut self) {
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
            self.review = None;
            self.edit.selected.clear();
            self.edit.alternatives.clear();
            self.notice = None;
        } else {
            self.close();
        }
    }
    pub fn ticket(&self) -> DraftTicket {
        DraftTicket {
            session: self.session,
            revision: self.revision,
        }
    }
    pub fn sources(&self) -> &[PathBuf] {
        &self.sources
    }
    pub fn destination(&self) -> Option<&std::path::Path> {
        self.destination.as_deref()
    }
    pub fn action_kind(&self) -> ActionKind {
        self.edit.action
    }
    pub fn verify(&self) -> bool {
        self.edit.verify
    }
    pub fn draft(&self) -> Option<TransferDraft> {
        let action = match self.edit.action {
            ActionKind::Copy => TransferAction::CopyTo(self.destination.clone()?),
            ActionKind::Move => TransferAction::MoveTo(self.destination.clone()?),
            ActionKind::Duplicate => TransferAction::Duplicate,
        };
        Some(TransferDraft {
            action,
            verify: self.edit.verify,
        })
    }
    pub fn review(&self) -> Option<&Arc<TransferReview>> {
        self.review.as_ref()
    }
    pub fn review_ticket(&self) -> Option<ReviewTicket> {
        self.review.as_ref().map(|review| ReviewTicket {
            draft: self.ticket(),
            generation: review.generation(),
        })
    }
    pub fn set_review(
        &mut self,
        ticket: DraftTicket,
        result: Result<TransferReview, String>,
    ) -> bool {
        if !self.open || self.ticket() != ticket {
            return false;
        }
        match result {
            Ok(review) if self.draft().as_ref() == Some(review.draft()) => {
                let same_source = self
                    .review
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old.source(), review.source()));
                if same_source {
                    let eligible: HashSet<_> = review
                        .rows()
                        .iter()
                        .filter(|row| is_conflict(row.state()))
                        .map(|row| row.source())
                        .collect();
                    self.edit.selected.retain(|node| eligible.contains(node));
                    self.edit
                        .alternatives
                        .retain(|node, _| eligible.contains(node));
                } else {
                    self.edit.selected.clear();
                    self.edit.alternatives.clear();
                }
                self.review = Some(Arc::new(review));
                self.notice = None;
            }
            Ok(_) => return false,
            Err(error) => {
                self.review = None;
                self.notice = Some(error);
            }
        }
        true
    }
    pub fn set_picker_result(
        &mut self,
        ticket: DraftTicket,
        kind: PickerKind,
        paths: Vec<PathBuf>,
    ) -> bool {
        if !self.open || self.ticket() != ticket || paths.is_empty() {
            return false;
        }
        match kind {
            PickerKind::Files => self.sources = paths,
            PickerKind::Folder => self.sources = vec![paths[0].clone()],
            PickerKind::Destination => self.destination = Some(paths[0].clone()),
        }
        self.changed();
        true
    }
    pub fn set_notice(&mut self, ticket: DraftTicket, message: String) -> bool {
        if !self.open || self.ticket() != ticket {
            return false;
        }
        self.notice = Some(message);
        true
    }
}

#[derive(Default)]
pub(super) struct Actions {
    scrolled_job: Option<TransferJobId>,
    session: u64,
    revision: u64,
    edit: Edit,
    close: bool,
    host_request: Option<UtilityPresentation>,
    intent: Option<Intent>,
}
#[derive(Debug)]
enum Intent {
    Pick(PickerKind),
    Review,
    Choice(SourceNodeId, ReviewChoice),
    Group(Vec<SourceNodeId>, GroupChoice),
    Queue,
    Start,
    Reorder(TransferJobId, Option<TransferJobId>),
    Pause(TransferJobId),
    Resume(TransferJobId),
    Cancel(TransferJobId),
    History(TransferHistoryQuery),
    ConfirmRecovery(u64, RecoveryVerb),
    CancelRecovery(u64),
}
impl HostedUtilityDialog for FileOperationsCenterDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::FileOperations);
    const KEY: &'static str = "file-operations-center";
    const TITLE: &'static str = "File Operations Center";
    const SIZE: egui::Vec2 = egui::vec2(800.0, 650.0);
    const MINIMUM: egui::Vec2 = egui::vec2(420.0, 320.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = Runtime;
    type Actions = Actions;
    type Checkpoint = ();
    type Command = Command;
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.session
    }
    fn parent_targets_allowed(&self, _: &Runtime) -> bool {
        true
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent: bool,
        _: NavigationOwner,
        _: bool,
        _: u64,
        runtime: &Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Actions {
        let mut actions = Actions {
            session: self.session,
            revision: self.revision,
            edit: self.edit.clone(),
            ..Default::default()
        };
        let mut open = true;
        let mut show = |ui: &mut egui::Ui| {
            ui.add_enabled_ui(parent, |ui| {
                self.contents(ui, &mut actions, runtime, scopes.0, presentation)
            });
        };
        if let Some(ui) = child {
            show(ui);
        } else {
            egui::Window::new(Self::TITLE)
                .id(Self::layer().id)
                .open(&mut open)
                .default_size(Self::SIZE)
                .min_size(Self::MINIMUM)
                .show(ctx, show);
        }
        actions.close |= !open;
        actions
    }
    fn host_request(actions: &Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Command {
        Command::Closed
    }
    fn command_closes(command: &Command) -> bool {
        matches!(command, Command::Closed)
    }
    fn complete_actions(&mut self, actions: Actions, hwnd: Option<usize>) -> Vec<Command> {
        if !self.open || actions.session != self.session || actions.revision != self.revision {
            return Vec::new();
        }
        if actions.scrolled_job.is_some() && actions.scrolled_job == self.focus_job {
            self.focus_job = None;
        }
        if actions.close {
            self.close();
            return vec![Command::Closed];
        }
        let draft_changed =
            self.edit.action != actions.edit.action || self.edit.verify != actions.edit.verify;
        self.edit = actions.edit;
        if draft_changed {
            self.changed();
            return Vec::new();
        }
        let ticket = self.ticket();
        let Some(intent) = actions.intent else {
            return Vec::new();
        };
        let command = match intent {
            Intent::Pick(kind) => match hwnd {
                Some(hwnd) => Command::Pick { ticket, kind, hwnd },
                None => {
                    self.notice = Some("This window cannot open a file picker right now.".into());
                    return Vec::new();
                }
            },
            Intent::Review => match self.draft() {
                Some(draft) if !self.sources.is_empty() => Command::Review {
                    ticket,
                    sources: self.sources.clone(),
                    draft,
                },
                _ => return Vec::new(),
            },
            Intent::Choice(node, choice) => match self.review_ticket() {
                Some(ticket) => Command::Choice {
                    ticket,
                    node,
                    choice,
                },
                None => return Vec::new(),
            },
            Intent::Group(nodes, choice) => match self.review_ticket() {
                Some(ticket) if !nodes.is_empty() => Command::Group {
                    ticket,
                    nodes,
                    choice,
                },
                _ => return Vec::new(),
            },
            Intent::Queue => match self.review_ticket() {
                Some(ticket) => Command::Queue { ticket },
                None => return Vec::new(),
            },
            Intent::Start => Command::Start,
            Intent::Reorder(job, before) => Command::Reorder { job, before },
            Intent::Pause(job) => Command::Pause(job),
            Intent::Resume(job) => Command::Resume(job),
            Intent::Cancel(job) => Command::Cancel(job),
            Intent::History(query) => Command::History(query),
            Intent::ConfirmRecovery(token, verb) => Command::ConfirmRecovery { token, verb },
            Intent::CancelRecovery(token) => Command::CancelRecovery { token },
        };
        vec![command]
    }
    #[cfg(test)]
    fn test_button(_: &Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}
fn is_conflict(state: &ReviewState) -> bool {
    matches!(state, ReviewState::Conflict(_))
}
fn conflict_class(
    review: &TransferReview,
    row: &crate::file_transfer::ReviewRow,
) -> Option<(EntryKind, EntryKind)> {
    let ReviewState::Conflict(target) = row.state() else {
        return None;
    };
    let TargetOccupancy::Occupied(target) = target.occupancy() else {
        return None;
    };
    let SourceObservation::Ready(source) = review.source().get(row.source())?.observation() else {
        return None;
    };
    Some((source.metadata().kind, target.metadata().kind))
}
fn review_state_label(state: &ReviewState) -> &'static str {
    match state {
        ReviewState::Vacant(_) => "Ready",
        ReviewState::Conflict(_) => "Choose an action",
        ReviewState::PlannedParent { .. } => "Parent will be created",
        ReviewState::WaitingParent { .. } => "Waiting for parent",
        ReviewState::Blocked(_) => "Blocked",
        ReviewState::Skipped => "Skipped",
        ReviewState::Replace(_) => "Replace",
        ReviewState::Merge { .. } => "Merge",
    }
}
fn job_details(ui: &mut egui::Ui, job: &crate::file_transfer::TransferJobInfo) {
    ui.label(format!("{:?} — {:?}", job.draft.action, job.status));
    for path in job.roots.iter() {
        ui.label(format!("Source: {}", path.display()));
    }
    match &job.draft.action {
        TransferAction::CopyTo(path) | TransferAction::MoveTo(path) => {
            ui.label(format!("Destination: {}", path.display()));
        }
        TransferAction::Duplicate => {
            ui.label("Destination: alongside source");
        }
    }
    if let Some(path) = &job.current_path {
        ui.label(format!("Current: {}", path.display()));
    }
    let bytes = job
        .progress
        .total_bytes
        .map_or_else(|| "unknown".into(), |n| n.to_string());
    ui.label(format!(
        "{:?} · files {} / {} · bytes {} / {} · verification {}",
        job.progress.phase,
        job.progress.completed_files,
        job.progress.total_files,
        job.progress.completed_bytes,
        bytes,
        if job.draft.verify {
            "requested"
        } else {
            "not requested"
        }
    ));
    if let Some(elapsed) = job.elapsed {
        ui.label(format!("Elapsed: {:.1}s", elapsed.as_secs_f64()));
    }
}
fn show_error_evidence(ui: &mut egui::Ui, error: &crate::file_transfer::CopyError) {
    use crate::file_transfer::CopyRecoveryArtifact;
    if let Some(artifact) = &error.artifact {
        match artifact {
            CopyRecoveryArtifact::Owned(file) => {
                ui.label(format!("Retained file: {}", file.path.display()));
            }
            CopyRecoveryArtifact::Relocation(effect) => {
                ui.label(format!(
                    "Move location: {:?} · {} → {}",
                    effect.location,
                    effect.source.display(),
                    effect.destination.display()
                ));
            }
            CopyRecoveryArtifact::StagedMove(effect) => {
                ui.label(format!(
                    "Published output: {}",
                    effect.output.path.display()
                ));
                ui.label(format!(
                    "Original source: {} · {:?}",
                    effect.source.display(),
                    effect.source_state
                ));
                if let Some(path) = &effect.recovery {
                    ui.label(format!("Source recovery name: {}", path.display()));
                }
            }
            CopyRecoveryArtifact::UncertainPublication { stage, target, .. } => {
                ui.label(format!(
                    "Uncertain publication: {} or {}",
                    stage.display(),
                    target.display()
                ));
            }
            CopyRecoveryArtifact::UnprovenStage { path, .. }
            | CopyRecoveryArtifact::UnprovenDirectory { path, .. } => {
                ui.label(format!("Unconfirmed artifact: {}", path.display()));
            }
        }
    }
    if let Some(backup) = &error.replacement {
        ui.label(format!(
            "Original backup: {} ({:?})",
            backup.backup.display(),
            backup.state
        ));
    }
}

fn show_result(ui: &mut egui::Ui, result: &TransferJobResult, page: &mut usize) {
    let report = match result {
        TransferJobResult::Finished(report) => Some(report),
        TransferJobResult::ReceiptFailed {
            report, failure, ..
        } => {
            ui.label(format!("Receipt recording failed: {failure:?}"));
            report.as_ref()
        }
        TransferJobResult::Rejected(error) => {
            ui.label(format!("Transfer rejected: {error}"));
            None
        }
        TransferJobResult::CancelledBeforeStart => {
            ui.label("Cancelled before start");
            None
        }
        TransferJobResult::WorkerPanicked => {
            ui.label("Transfer worker stopped unexpectedly");
            None
        }
    };
    if let Some(report) = report {
        let pages = report.rows.len().div_ceil(50).max(1);
        *page = (*page).min(pages - 1);
        if pages > 1 {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(*page > 0, egui::Button::new("Previous results"))
                    .clicked()
                {
                    *page -= 1;
                }
                ui.label(format!("Results {} / {}", *page + 1, pages));
                if ui
                    .add_enabled(*page + 1 < pages, egui::Button::new("Next results"))
                    .clicked()
                {
                    *page += 1;
                }
            });
        }
        for row in report.rows.iter().skip(*page * 50).take(50) {
            match &row.state {
                ExecutionState::Copied(copy) => {
                    ui.label(format!(
                        "Copied: {} · {} bytes · {}",
                        copy.output.path.display(),
                        copy.bytes,
                        match copy.verification {
                            CopyVerification::NotRequested => "not verified",
                            CopyVerification::Verified { .. } => "verified",
                        }
                    ));
                    if let Some(backup) = &copy.replacement {
                        ui.label(format!(
                            "Original backup: {} ({:?})",
                            backup.backup.display(),
                            backup.state
                        ));
                    }
                }
                ExecutionState::Failed(error) => {
                    ui.label(format!("Failed: {error}"));
                    show_error_evidence(ui, error);
                }
                ExecutionState::Cancelled(error) => {
                    ui.label("Cancelled");
                    if let Some(error) = error {
                        show_error_evidence(ui, error);
                    }
                }
                ExecutionState::Skipped => {
                    ui.label("Skipped");
                }
                ExecutionState::Moved(moved) => {
                    ui.label(format!(
                        "Moved: {} → {} · {} bytes · {:?}",
                        moved.effect.source.display(),
                        moved.effect.destination.display(),
                        moved.bytes,
                        moved.effect.verification
                    ));
                }
                ExecutionState::MovedByCopy(moved) => {
                    ui.label(format!(
                        "Moved by verified copy: {} → {} · {} bytes",
                        moved.effect.source.display(),
                        moved.effect.output.path.display(),
                        moved.bytes
                    ));
                    if let Some(backup) = &moved.effect.replacement {
                        ui.label(format!(
                            "Original backup: {} ({:?})",
                            backup.backup.display(),
                            backup.state
                        ));
                    }
                }
                ExecutionState::MovedDirectory(_) => {
                    ui.label("Directory move result recorded");
                }
                ExecutionState::CreatedDirectory(dir) => {
                    ui.label(format!("Directory created: {}", dir.output.path.display()));
                }
                ExecutionState::MergedDirectory(_) => {
                    ui.label("Directory merged");
                }
                ExecutionState::BlockedByParent(_) => {
                    ui.label("Blocked by parent");
                }
                ExecutionState::BlockedByReceipt => {
                    ui.label("Blocked by receipt recording");
                }
            }
        }
    }
}

impl FileOperationsCenterDialog {
    fn scroll_to_activity_job(&self, ui: &mut egui::Ui, actions: &mut Actions, job: TransferJobId) {
        if self.focus_job == Some(job) {
            ui.label(format!("Activity job {}", job.get()))
                .scroll_to_me(Some(egui::Align::Center));
            actions.scrolled_job = Some(job);
        }
    }

    fn contents(
        &self,
        ui: &mut egui::Ui,
        actions: &mut Actions,
        runtime: &Runtime,
        scope: Option<&JumpModeScope>,
        presentation: UtilityPresentation,
    ) {
        let mut order = 0;
        macro_rules! target {
            ($ui:expr, $response:expr, $id:expr, $label:expr, $kind:expr) => {
                register_and_apply_target(scope, $ui, &$response, $id, $label, $kind, &mut order);
            };
        }
        ui.horizontal_wrapped(|ui| {
            let (button, next) = presentation_button(ui, presentation, ui.is_enabled());
            target!(
                ui,
                button,
                "presentation",
                "Change window presentation",
                TargetKind::Button
            );
            if button.clicked() {
                actions.host_request = Some(next);
            }
            let close = ui.button("Close");
            target!(ui, close, "close", "Close", TargetKind::Button);
            actions.close |= close.clicked();
        });
        egui::ScrollArea::vertical()
            .id_salt("file-operations-content")
            .show(ui, |ui| {
                if let Some(message) = &self.notice {
                    ui.label(message);
                }
                ui.heading("Transfer draft");
                ui.horizontal_wrapped(|ui| {
                    for (kind, label, id) in [
                        (ActionKind::Copy, "Copy", "copy"),
                        (ActionKind::Move, "Move", "move"),
                        (ActionKind::Duplicate, "Duplicate", "duplicate"),
                    ] {
                        let response = ui.radio_value(&mut actions.edit.action, kind, label);
                        target!(ui, response, id, label, TargetKind::Selectable);
                    }
                });
                let verify = ui.checkbox(&mut actions.edit.verify, "Verify copied data");
                target!(
                    ui,
                    verify,
                    "verify",
                    "Verify copied data",
                    TargetKind::Checkbox
                );
                ui.horizontal_wrapped(|ui| {
                    for (kind, label, id) in [
                        (PickerKind::Files, "Choose files…", "files"),
                        (PickerKind::Folder, "Choose folder…", "folder"),
                        (
                            PickerKind::Destination,
                            "Choose destination…",
                            "destination",
                        ),
                    ] {
                        let response = ui.button(label);
                        target!(ui, response, id, label, TargetKind::Button);
                        if response.clicked() {
                            actions.intent = Some(Intent::Pick(kind));
                        }
                    }
                });
                ui.label(format!("Sources: {}", self.sources.len()));
                for path in &self.sources {
                    ui.label(path.display().to_string());
                }
                if let Some(path) = &self.destination {
                    ui.label(format!("Destination: {}", path.display()));
                }
                let ready = !runtime.busy
                    && !self.sources.is_empty()
                    && (actions.edit.action == ActionKind::Duplicate || self.destination.is_some());
                let review = ui.add_enabled(ready, egui::Button::new("Review"));
                target!(ui, review, "review", "Review transfer", TargetKind::Button);
                if review.clicked() {
                    actions.intent = Some(Intent::Review);
                }
                if let Some(review) = &self.review {
                    ui.separator();
                    ui.heading("Review");
                    ui.label("Replacing a file retains the original backup; generic Undo is unavailable.");
                    let mut selected: Vec<_> = review
                        .rows()
                        .iter()
                        .filter(|r| is_conflict(r.state()) && actions.edit.selected.contains(&r.source()))
                        .map(|r| r.source())
                        .collect();
                    let mut selected_class = None;
                    let mut compatible = true;
                    for row in review.rows().iter().filter(|row| actions.edit.selected.contains(&row.source())) {
                        let class = conflict_class(review, row);
                        if class.is_none() || selected_class.is_some_and(|previous| Some(previous) != class) { compatible = false; }
                        if selected_class.is_none() { selected_class = class; }
                    }
                    ui.horizontal_wrapped(|ui| {
                        for (choice, label, id) in [
                            (GroupChoice::Skip, "Skip selected", "group-skip"),
                            (GroupChoice::Replace, "Replace selected", "group-replace"),
                        ] {
                            let response = ui.add_enabled(
                                !selected.is_empty() && compatible && !runtime.busy && (matches!(choice, GroupChoice::Skip) || selected_class == Some((EntryKind::File, EntryKind::File))),
                                egui::Button::new(label),
                            );
                            target!(ui, response, id, label, TargetKind::Button);
                            if response.clicked() {
                                actions.intent =
                                    Some(Intent::Group(std::mem::take(&mut selected), choice));
                            }
                        }
                    });
                    const PAGE_ROWS: usize = 50;
                    let pages = review.rows().len().div_ceil(PAGE_ROWS).max(1);
                    actions.edit.review_page = actions.edit.review_page.min(pages - 1);
                    ui.horizontal(|ui| {
                        let prev = ui.add_enabled(actions.edit.review_page > 0, egui::Button::new("Previous review page"));
                        target!(ui, prev, "review/page/previous", "Previous review page", TargetKind::Button);
                        if prev.clicked() { actions.edit.review_page -= 1; }
                        ui.label(format!("Page {} / {}", actions.edit.review_page + 1, pages));
                        let next = ui.add_enabled(actions.edit.review_page + 1 < pages, egui::Button::new("Next review page"));
                        target!(ui, next, "review/page/next", "Next review page", TargetKind::Button);
                        if next.clicked() { actions.edit.review_page += 1; }
                    });
                    for row in review.rows().iter().skip(actions.edit.review_page * PAGE_ROWS).take(PAGE_ROWS) {
                        let node = row.source();
                        let id = format!("{:?}", node);
                        ui.push_id(node, |ui| {
                            let mut selected = actions.edit.selected.contains(&node);
                            let source_path = review.source().get(node).map(|entry| entry.path().display().to_string()).unwrap_or_else(|| "Source unavailable".into());
                            let target_path = row.destination_path().map(|path| path.display().to_string()).unwrap_or_else(|| "Destination unavailable".into());
                            let select = ui.add_enabled(is_conflict(row.state()), egui::Checkbox::new(&mut selected,
                                format!("{} → {} — {}", source_path, target_path, review_state_label(row.state()))));
                            target!(
                                ui,
                                select,
                                &format!("row/{id}/select"),
                                "Select review row",
                                TargetKind::Checkbox
                            );
                            if select.changed() {
                                if selected {
                                    actions.edit.selected.insert(node);
                                } else {
                                    actions.edit.selected.remove(&node);
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                for (choice, label, key) in [
                                    (ReviewChoice::Reset, "Reset", "reset"),
                                    (ReviewChoice::Skip, "Skip", "skip"),
                                    (ReviewChoice::Replace, "Replace", "replace"),
                                    (ReviewChoice::Merge, "Merge", "merge"),
                                ] {
                                    let source_kind = review.source().get(node).and_then(|entry| match entry.observation() { SourceObservation::Ready(snapshot) => Some(snapshot.metadata().kind), _ => None });
                                    let target_kind = match row.state() {
                                        ReviewState::Conflict(target) | ReviewState::Replace(target) => match target.occupancy() { TargetOccupancy::Occupied(entry) => Some(entry.metadata().kind), _ => None },
                                        ReviewState::Merge { .. } => Some(EntryKind::Directory), _ => None,
                                    };
                                    let enabled = !runtime.busy && match &choice {
                                        ReviewChoice::Reset | ReviewChoice::Skip => true,
                                        ReviewChoice::Replace => source_kind == Some(EntryKind::File) && target_kind == Some(EntryKind::File),
                                        ReviewChoice::Merge => source_kind == Some(EntryKind::Directory) && target_kind == Some(EntryKind::Directory),
                                        ReviewChoice::KeepBoth(_) => false,
                                    };
                                    let response =
                                        ui.add_enabled(enabled, egui::Button::new(label));
                                    target!(
                                        ui,
                                        response,
                                        &format!("row/{id}/{key}"),
                                        label,
                                        TargetKind::Button
                                    );
                                    if response.clicked() {
                                        actions.intent = Some(Intent::Choice(node, choice));
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                let mut name = actions.edit.alternatives.get(&node).cloned().unwrap_or_default();
                                let field = ui.add(
                                    egui::TextEdit::singleline(&mut name)
                                        .hint_text("Alternative filename")
                                        .char_limit(255),
                                );
                                target!(
                                    ui,
                                    field,
                                    &format!("row/{id}/name"),
                                    "Alternative filename",
                                    TargetKind::TextEntry
                                );
                                if field.changed() {
                                    if name.is_empty() { actions.edit.alternatives.remove(&node); }
                                    else { actions.edit.alternatives.insert(node, name.clone()); }
                                }
                                let choice = TransferLeaf::new(OsStr::new(&name));
                                let button = ui.add_enabled(
                                    !runtime.busy && is_conflict(row.state()) && choice.is_ok(),
                                    egui::Button::new("Keep both"),
                                );
                                target!(
                                    ui,
                                    button,
                                    &format!("row/{id}/keep"),
                                    "Keep both",
                                    TargetKind::Button
                                );
                                if button.clicked() {
                                    if let Ok(leaf) = choice {
                                        actions.intent = Some(Intent::Choice(
                                            node,
                                            ReviewChoice::KeepBoth(leaf),
                                        ));
                                    }
                                }
                            });
                        });
                    }
                    let queue =
                        ui.add_enabled(!runtime.busy, egui::Button::new("Queue reviewed transfer"));
                    target!(
                        ui,
                        queue,
                        "queue",
                        "Queue reviewed transfer",
                        TargetKind::Button
                    );
                    if queue.clicked() {
                        actions.intent = Some(Intent::Queue);
                    }
                }
                ui.separator();
                ui.heading("Queue");
                let start =
                    ui.add_enabled(!runtime.queue.queued.is_empty(), egui::Button::new("Start"));
                target!(ui, start, "start", "Start queue", TargetKind::Button);
                if start.clicked() {
                    actions.intent = Some(Intent::Start);
                }
                for (index, job) in runtime.queue.queued.iter().enumerate() {
                    self.scroll_to_activity_job(ui, actions, job.id);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{:?} — {:?}", job.draft.action, job.status));
                        if let Some(path) = job.roots.first() {
                            ui.label(format!("Source: {}", path.display()));
                        }
                        if job.roots.len() > 1 {
                            ui.collapsing(format!("All {} sources", job.roots.len()), |ui| {
                                for path in job.roots.iter() {
                                    ui.label(path.display().to_string());
                                }
                            });
                        }
                        if index > 0 {
                            let response = ui.button("Move up");
                            target!(
                                ui,
                                response,
                                &format!("job/{}/up", job.id.get()),
                                "Move queued job up",
                                TargetKind::Button
                            );
                            if response.clicked() {
                                actions.intent = Some(Intent::Reorder(
                                    job.id,
                                    Some(runtime.queue.queued[index - 1].id),
                                ));
                            }
                        }
                        if index + 1 < runtime.queue.queued.len() {
                            let response = ui.button("Move down");
                            target!(
                                ui,
                                response,
                                &format!("job/{}/down", job.id.get()),
                                "Move queued job down",
                                TargetKind::Button
                            );
                            if response.clicked() {
                                actions.intent = Some(Intent::Reorder(
                                    job.id,
                                    runtime.queue.queued.get(index + 2).map(|next| next.id),
                                ));
                            }
                        }
                        let cancel = ui.button("Cancel");
                        target!(
                            ui,
                            cancel,
                            &format!("job/{}/cancel", job.id.get()),
                            "Cancel queued job",
                            TargetKind::Button
                        );
                        if cancel.clicked() {
                            actions.intent = Some(Intent::Cancel(job.id));
                        }
                    });
                }
                if let Some(job) = &runtime.queue.active {
                    self.scroll_to_activity_job(ui, actions, job.id);
                    job_details(ui, job);
                    ui.horizontal_wrapped(|ui| {
                        if matches!(job.status, TransferJobStatus::Paused | TransferJobStatus::PauseRequested) {
                            let b = ui.button("Resume"); target!(ui, b, "active/resume", "Resume transfer", TargetKind::Button);
                            if b.clicked() { actions.intent = Some(Intent::Resume(job.id)); }
                        } else {
                            let b = ui.add_enabled(!matches!(job.status, TransferJobStatus::Cancelling), egui::Button::new("Pause")); target!(ui, b, "active/pause", "Pause transfer", TargetKind::Button);
                            if b.clicked() { actions.intent = Some(Intent::Pause(job.id)); }
                        }
                        let b = ui.add_enabled(job.status != TransferJobStatus::Cancelling, egui::Button::new("Cancel")); target!(ui, b, "active/cancel", "Cancel transfer", TargetKind::Button);
                        if b.clicked() { actions.intent = Some(Intent::Cancel(job.id)); }
                    });
                }
                for job in &runtime.queue.history {
                    self.scroll_to_activity_job(ui, actions, job.id);
                    ui.separator(); job_details(ui, job);
                    if let Some(result) = &job.result {
                        let mut page = actions.edit.terminal_page
                            .filter(|(id, _)| *id == job.id)
                            .map_or(0, |(_, page)| page);
                        let before = page;
                        ui.push_id(("result", job.id.get()), |ui| show_result(ui, result, &mut page));
                        if page != before {
                            actions.edit.terminal_page = Some((job.id, page));
                        }
                    }
                    if let Some(id) = job.receipt_id {
                        let b = ui.add_enabled(!runtime.history_waiting, egui::Button::new("Inspect receipt"));
                        target!(ui, b, &format!("job/{}/receipt", job.id.get()), "Inspect receipt", TargetKind::Button);
                        if b.clicked() { actions.intent = Some(Intent::History(TransferHistoryQuery::Detail(id))); }
                    }
                }
                if runtime.queue.persistence_uncertain || runtime.queue.newer_snapshot_incomplete {
                    ui.label("Queue persistence needs attention.");
                }
                for job in &runtime.queue.interrupted {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Interrupted: {} → {}", job.source, job.destination));
                        let b = ui.add_enabled(!runtime.history_waiting, egui::Button::new("Discard interrupted preview"));
                        target!(ui, b, &format!("interrupted/{}/discard", job.receipt_id), "Discard interrupted preview", TargetKind::Button);
                        if b.clicked() { actions.intent = Some(Intent::History(TransferHistoryQuery::Discard(job.receipt_id))); }
                    });
                }
                if let Some(preview) = &runtime.recovery {
                    ui.separator();
                    ui.heading(match preview.verb {
                        RecoveryVerb::Retry => "Review retry",
                        RecoveryVerb::Undo => "Review undo",
                    });
                    ui.label(format!("Source: {}", preview.source));
                    ui.label(format!("Destination: {}", preview.destination));
                    let label = match preview.verb {
                        RecoveryVerb::Retry => "Queue retry",
                        RecoveryVerb::Undo => "Queue undo",
                    };
                    let accept = ui.add_enabled(!runtime.busy, egui::Button::new(label));
                    target!(ui, accept, "recovery/confirm", label, TargetKind::Button);
                    if accept.clicked() {
                        actions.intent = Some(Intent::ConfirmRecovery(preview.token, preview.verb));
                    }
                    let cancel = ui.button("Cancel recovery");
                    target!(
                        ui,
                        cancel,
                        "recovery/cancel",
                        "Cancel recovery",
                        TargetKind::Button
                    );
                    if cancel.clicked() {
                        actions.intent = Some(Intent::CancelRecovery(preview.token));
                    }
                }
                ui.separator();
                ui.heading("History");
                if runtime.history_waiting { ui.label("Loading receipt history…"); }
                let history = ui.add_enabled(!runtime.history_waiting, egui::Button::new("Refresh history"));
                target!(
                    ui,
                    history,
                    "history/refresh",
                    "Refresh history",
                    TargetKind::Button
                );
                if history.clicked() {
                    actions.intent = Some(Intent::History(TransferHistoryQuery::Summaries));
                }
                for item in runtime.history.iter() {
                    if let Some(id) = item.id {
                        let b = ui.add_enabled(!runtime.history_waiting, egui::Button::new(format!("{} — {:?}", id, item.health)));
                        target!(
                            ui,
                            b,
                            &format!("history/{id}"),
                            "Receipt details",
                            TargetKind::Button
                        );
                        if b.clicked() {
                            actions.intent =
                                Some(Intent::History(TransferHistoryQuery::Detail(id)));
                        }
                    } else {
                        ui.label(format!("Receipt details unavailable · {:?}", item.health));
                    }
                }
                if let Some(detail) = &runtime.detail {
                    if let Some(id) = detail.summary.id {
                        self.receipt_details(ui, actions, scope, &mut order, id, detail, runtime.history_waiting);
                    }
                }
            });
    }
    fn receipt_details(
        &self,
        ui: &mut egui::Ui,
        actions: &mut Actions,
        scope: Option<&JumpModeScope>,
        order: &mut u64,
        id: TransferReceiptId,
        detail: &ReceiptDetail,
        waiting: bool,
    ) {
        ui.label(format!("Receipt {id}: {:?}", detail.summary.health));
        const PAGE_ROWS: usize = 50;
        let pages = detail.entries.len().div_ceil(PAGE_ROWS).max(1);
        actions.edit.detail_page = actions.edit.detail_page.min(pages - 1);
        ui.horizontal(|ui| {
            let prev = ui.add_enabled(
                actions.edit.detail_page > 0,
                egui::Button::new("Previous receipt page"),
            );
            register_and_apply_target(
                scope,
                ui,
                &prev,
                "receipt/page/previous",
                "Previous receipt page",
                TargetKind::Button,
                order,
            );
            if prev.clicked() {
                actions.edit.detail_page -= 1;
            }
            ui.label(format!("Page {} / {}", actions.edit.detail_page + 1, pages));
            let next = ui.add_enabled(
                actions.edit.detail_page + 1 < pages,
                egui::Button::new("Next receipt page"),
            );
            register_and_apply_target(
                scope,
                ui,
                &next,
                "receipt/page/next",
                "Next receipt page",
                TargetKind::Button,
                order,
            );
            if next.clicked() {
                actions.edit.detail_page += 1;
            }
        });
        for entry in detail
            .entries
            .iter()
            .skip(actions.edit.detail_page * PAGE_ROWS)
            .take(PAGE_ROWS)
        {
            ui.group(|ui| {
                ui.label(format!("Entry {} — {:?}", entry.index, entry.status));
                match &entry.intended_target {
                    ReceiptTarget::Direct { path, choice, .. } => {
                        ui.label(format!("Target: {} ({choice:?})", path.display()));
                    }
                    ReceiptTarget::Child { parent_node, leaf } => {
                        ui.label(format!(
                            "Target: child of entry {parent_node} / {}",
                            leaf.to_string_lossy()
                        ));
                    }
                    ReceiptTarget::Skipped => {
                        ui.label("Target: skipped");
                    }
                }
                for evidence in &entry.evidence {
                    ui.label(format!(
                        "{:?}: {} — {:?}",
                        evidence.role,
                        evidence.path.display(),
                        evidence.observation
                    ));
                }
                let source_current = entry.evidence.iter().any(|e| {
                    e.role == ReceiptEvidenceRole::Source
                        && e.observation == ReceiptObservation::SameRevision
                        && e.recorded_revision
                            .is_some_and(|revision| revision.kind == EntryKind::File)
                });
                let retained_effect = entry.evidence.iter().any(|e| {
                    matches!(
                        e.role,
                        ReceiptEvidenceRole::Output
                            | ReceiptEvidenceRole::Stage
                            | ReceiptEvidenceRole::Backup
                            | ReceiptEvidenceRole::Quarantine
                    ) && !matches!(
                        e.observation,
                        ReceiptObservation::Vacant | ReceiptObservation::Unassigned
                    )
                });
                let retry = detail.trusted_prefix
                    && detail.outline_complete
                    && source_current
                    && !retained_effect
                    && matches!(
                        entry.status,
                        ReceiptEntryStatus::Remaining
                            | ReceiptEntryStatus::Failed
                            | ReceiptEntryStatus::Cancelled
                            | ReceiptEntryStatus::Blocked
                    );
                let undo = detail.trusted_prefix
                    && detail.outline_complete
                    && detail.summary.health == ReceiptHealth::Completed
                    && entry.verified_atomic_move.is_some();
                for (label, query, key, enabled) in [
                    (
                        "Retry",
                        TransferHistoryQuery::Retry(id, entry.index),
                        "retry",
                        retry,
                    ),
                    (
                        "Undo",
                        TransferHistoryQuery::Undo(id, entry.index),
                        "undo",
                        undo,
                    ),
                ] {
                    let b = ui.add_enabled(enabled && !waiting, egui::Button::new(label));
                    register_and_apply_target(
                        scope,
                        ui,
                        &b,
                        &format!("receipt/{id}/{}/{key}", entry.index),
                        label,
                        TargetKind::Button,
                        order,
                    );
                    if b.clicked() {
                        actions.intent = Some(Intent::History(query));
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_operations_center_picker_requires_current_owner() {
        let mut dialog = FileOperationsCenterDialog::default();
        dialog.open();
        let actions = Actions {
            session: dialog.session,
            revision: dialog.revision,
            edit: dialog.edit.clone(),
            intent: Some(Intent::Pick(PickerKind::Files)),
            ..Default::default()
        };
        assert!(dialog.complete_actions(actions, None).is_empty());
        assert!(dialog.notice.is_some());
    }
    #[test]
    fn file_operations_center_stale_picker_and_close_preserve_draft() {
        let mut dialog = FileOperationsCenterDialog::default();
        dialog.open_with_sources(vec![PathBuf::from("first")]);
        let old = dialog.ticket();
        assert!(dialog.set_picker_result(
            old,
            PickerKind::Destination,
            vec![PathBuf::from("target")]
        ));
        assert!(!dialog.set_picker_result(old, PickerKind::Files, vec![PathBuf::from("wrong")]));
        dialog.close();
        dialog.open();
        assert_eq!(dialog.sources(), &[PathBuf::from("first")]);
        assert_eq!(dialog.destination(), Some(std::path::Path::new("target")));
        assert!(!dialog.set_picker_result(old, PickerKind::Files, vec![PathBuf::from("wrong")]));
    }
}
