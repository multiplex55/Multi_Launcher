use crate::file_lock::{
    FileLockInspection, FileLockProcess, FileLockSession, FileLockSessionStatus,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::gui::utility_host::ViewportPassProvenance;
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const WINDOW_TITLE: &str = "File Lock Inspector";
const UTILITY_KEY: &str = "file-lock";
const ROW_HEIGHT: f32 = 24.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileLockRevealRequest {
    pub(crate) generation: u64,
    pub(crate) process_id: u32,
    pub(crate) creation_filetime: u64,
    pub(crate) executable_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileLockDialogEffect {
    CopySnapshot(String),
    RevealExecutable(FileLockRevealRequest),
    FocusPanel,
}

#[derive(Clone)]
enum FileLockAction {
    Open(Option<PathBuf>),
    Prefill(PathBuf),
    ChooseFile,
    Inspect(PathBuf),
    Refresh {
        generation: u64,
        path: PathBuf,
    },
    Cancel {
        generation: u64,
    },
    SelectProcess {
        generation: u64,
        process_id: u32,
        creation_filetime: u64,
    },
    CopySnapshot {
        generation: u64,
    },
    Reveal(FileLockRevealRequest),
    Close,
}

struct PendingFileLockPass {
    provenance: ViewportPassProvenance,
    actions: Vec<FileLockAction>,
}

#[derive(Clone)]
struct SubmittedInspection {
    generation: u64,
    path: PathBuf,
}

struct RevealWorker {
    request: FileLockRevealRequest,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<crate::file_lock::ValidatedFileLockExecutable, String>>,
    joined: Arc<AtomicBool>,
    retirement_unavailable: Arc<AtomicBool>,
}

struct ReadyReveal {
    request: FileLockRevealRequest,
    executable: Option<crate::file_lock::ValidatedFileLockExecutable>,
    dispatched: bool,
}

struct RevealRetirementReceipt {
    joined: Arc<AtomicBool>,
    unavailable: Arc<AtomicBool>,
    context: egui::Context,
    resolved: bool,
}

impl RevealRetirementReceipt {
    fn joined(mut self) {
        self.resolved = true;
        self.joined.store(true, Ordering::Release);
        self.context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

impl Drop for RevealRetirementReceipt {
    fn drop(&mut self) {
        if !self.resolved {
            self.unavailable.store(true, Ordering::Release);
            self.context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

pub(crate) struct FileLockDialogState {
    pub(crate) open: bool,
    session: FileLockSession,
    selected_path: Option<PathBuf>,
    submitted: Option<SubmittedInspection>,
    selected_process: Option<(u64, u32, u64)>,
    pending: Option<PendingFileLockPass>,
    latest_pass: Option<ViewportPassProvenance>,
    notice: Option<String>,
    reveal_worker: Option<RevealWorker>,
    reveal_ready: Option<ReadyReveal>,
    reveal_retirement_failed: bool,
    #[cfg(test)]
    test_file_choice: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_file_picker_calls: usize,
    #[cfg(test)]
    test_reveal_result: Option<Result<PathBuf, String>>,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
}

impl FileLockDialogState {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let wake_context = ctx.clone();
        Self {
            open: false,
            session: FileLockSession::new(move || {
                wake_context.request_repaint_of(egui::ViewportId::ROOT);
            }),
            selected_path: None,
            submitted: None,
            selected_process: None,
            pending: None,
            latest_pass: None,
            notice: None,
            reveal_worker: None,
            reveal_ready: None,
            reveal_retirement_failed: false,
            #[cfg(test)]
            test_file_choice: None,
            #[cfg(test)]
            test_file_picker_calls: 0,
            #[cfg(test)]
            test_reveal_result: None,
            #[cfg(test)]
            test_action_rects: Vec::new(),
        }
    }

    pub(crate) fn request_open(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.stage(ctx, vec![FileLockAction::Open(path)]);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn stage_prefill(&mut self, ctx: &egui::Context, path: PathBuf) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.stage(ctx, vec![FileLockAction::Prefill(path)]);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn request_close(&mut self, ctx: &egui::Context) {
        if self.open || self.pending.is_some() {
            self.stage(ctx, vec![FileLockAction::Close]);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.open = false;
        self.pending = None;
        self.invalidate_selection();
        self.cancel_reveal();
        self.session.close();
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.pending.is_some()
            || self.session.view().is_busy()
            || self.reveal_worker.is_some()
            || self.reveal_ready.is_some()
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
        input_allowed: bool,
        first_ui_pass: bool,
    ) {
        self.poll_reveal_work();
        if !self.open {
            return;
        }
        #[cfg(test)]
        self.test_action_rects.clear();

        let pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        self.latest_pass = Some(pass);
        let commit_allowed = input_allowed
            && root_visible
            && first_ui_pass
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && !egui::Popup::is_any_open(ctx);
        let view = self.session.view();
        let current_inspection = self.current_inspection(&view);
        let selected_path = self.selected_path.clone();
        let selected_process = self.selected_process;
        let notice = self.notice.clone();
        let reveal_busy = self.reveal_worker.is_some() || self.reveal_retirement_failed;
        let mut window_open = self.open;
        let mut actions = Vec::new();

        egui::Window::new(WINDOW_TITLE)
            .id(egui::Id::new(Some(WINDOW_TITLE)))
            .collapsible(false)
            .resizable(true)
            .default_size((760.0, 520.0))
            .min_size((520.0, 340.0))
            .max_size(ctx.content_rect().size())
            .show_utility(
                ctx,
                UTILITY_KEY,
                egui::Id::new(WINDOW_TITLE),
                Some(&mut window_open),
                crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                    crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                        egui::vec2(760.0, 520.0),
                    ),
                    egui::vec2(520.0, 340.0),
                    [None, None],
                )
                .with_maximum(ctx.content_rect().size()),
                |ui| {
                    let controls_allowed = commit_allowed && ui.is_visible() && !ui.is_sizing_pass();
                    ui.add_enabled_ui(controls_allowed, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let choose = ui.button("Choose file…");
                            self.record_action_rect("choose_file", choose.rect, ui.clip_rect());
                            if choose.clicked() {
                                actions.push(FileLockAction::ChooseFile);
                            }

                            let inspect = ui.add_enabled(
                                selected_path.is_some() && !view.is_busy(),
                                egui::Button::new("Inspect"),
                            );
                            self.record_action_rect("inspect", inspect.rect, ui.clip_rect());
                            if inspect.clicked()
                                && let Some(path) = selected_path.clone()
                            {
                                actions.push(FileLockAction::Inspect(path));
                            }

                            let refresh = ui.add_enabled(
                                selected_path.is_some() && !view.is_busy(),
                                egui::Button::new("Refresh"),
                            );
                            self.record_action_rect("refresh", refresh.rect, ui.clip_rect());
                            if refresh.clicked()
                                && let Some(path) = selected_path.clone()
                            {
                                actions.push(FileLockAction::Refresh {
                                    generation: view.generation(),
                                    path,
                                });
                            }

                            let cancel = ui.add_enabled(
                                view.is_busy(),
                                egui::Button::new("Cancel"),
                            );
                            self.record_action_rect("cancel", cancel.rect, ui.clip_rect());
                            if cancel.clicked() {
                                actions.push(FileLockAction::Cancel {
                                    generation: view.generation(),
                                });
                            }

                            let can_copy = current_inspection.is_some();
                            let copy = ui.add_enabled(
                                can_copy,
                                egui::Button::new("Copy snapshot"),
                            );
                            self.record_action_rect("copy_snapshot", copy.rect, ui.clip_rect());
                            if copy.clicked() {
                                actions.push(FileLockAction::CopySnapshot {
                                    generation: view.generation(),
                                });
                            }
                        });

                        if let Some(path) = selected_path.as_ref() {
                            ui.label(format!("Selected file: {}", path.display()));
                        } else {
                            ui.label("Choose a regular file, then select Inspect.");
                        }

                        let status_view = self
                            .submitted_matches_view(&view)
                            .then_some(&view);
                        show_inspection_status(ui, status_view, current_inspection);

                        if let Some(inspection) = current_inspection.as_deref() {
                            if inspection.reports_no_processes() {
                                ui.weak("No processes reported.");
                            } else {
                                if inspection.processes.iter().any(process_details_unavailable) {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        "Some process details are unavailable; reported rows are retained.",
                                    );
                                }
                                let selected = selected_process.and_then(|(generation, pid, creation)| {
                                    (generation == view.generation())
                                        .then(|| {
                                            inspection.processes.iter().find(|process| {
                                                process.process_id == pid
                                                    && process.creation_filetime == creation
                                            })
                                        })
                                        .flatten()
                                });
                                let reveal_path = selected.and_then(|process| process.executable_path.as_ref());
                                let reveal = ui.add_enabled(
                                    reveal_path.is_some() && !reveal_busy,
                                    egui::Button::new("Reveal executable"),
                                );
                                self.record_action_rect("reveal", reveal.rect, ui.clip_rect());
                                if reveal.clicked()
                                    && let Some(process) = selected
                                    && let Some(executable_path) = process.executable_path.clone()
                                {
                                    actions.push(FileLockAction::Reveal(FileLockRevealRequest {
                                        generation: view.generation(),
                                        process_id: process.process_id,
                                        creation_filetime: process.creation_filetime,
                                        executable_path,
                                    }));
                                }
                                egui::ScrollArea::vertical()
                                    .id_salt("file_lock_process_rows")
                                    .max_height(
                                        (ui.available_height() - 52.0).max(ROW_HEIGHT * 3.0),
                                    )
                                    .show(ui, |ui| {
                                        for process in &inspection.processes {
                                            let key = (
                                                view.generation(),
                                                process.process_id,
                                                process.creation_filetime,
                                            );
                                            let selected = selected_process == Some(key);
                                            let label = format!(
                                                "PID {} • {}",
                                                process.process_id,
                                                process_label(process),
                                            );
                                            let response = ui.add_sized(
                                                [ui.available_width(), ROW_HEIGHT],
                                                egui::Button::selectable(selected, label),
                                            );
                                            self.record_action_rect(
                                                "process_row",
                                                response.rect,
                                                ui.clip_rect(),
                                            );
                                            if response.clicked() && controls_allowed {
                                                actions.push(FileLockAction::SelectProcess {
                                                    generation: view.generation(),
                                                    process_id: process.process_id,
                                                    creation_filetime: process.creation_filetime,
                                                });
                                            }
                                            ui.horizontal_wrapped(|ui| {
                                                ui.small(format!(
                                                    "Started {} • FILETIME {}",
                                                    format_process_start(process.creation_filetime),
                                                    process.creation_filetime,
                                                ));
                                                if !process.service_name.is_empty() {
                                                    ui.small(format!("Service: {}", process.service_name));
                                                }
                                            });
                                            if let Some(executable) = process.executable_path.as_ref() {
                                                ui.small(format!("Executable: {}", executable.display()));
                                            } else {
                                                ui.small("Executable path unavailable.");
                                            }
                                            ui.separator();
                                        }
                                    });

                            }
                            ui.weak("Windows reports these users; the list does not prove which process is denying access.");
                        }

                        if let Some(notice) = notice.as_ref() {
                            ui.colored_label(ui.visuals().warn_fg_color, notice);
                        }
                    });
                },
            );

        if !window_open && commit_allowed {
            actions.push(FileLockAction::Close);
        }
        self.stage_with_provenance(pass, actions);
    }

    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<FileLockDialogEffect> {
        self.poll_reveal_work();
        let mut effects = Vec::new();
        if let Some(pending) = self.pending.take() {
            if !pending.provenance.was_retained(ctx) {
                if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                    > pending.provenance.frame_nr
                {
                    self.pending = None;
                } else {
                    self.pending = Some(pending);
                }
            } else {
                for action in pending.actions {
                    match action {
                        FileLockAction::Open(path) => {
                            self.open = true;
                            if let Some(path) = path {
                                self.apply_selected_path(path);
                            }
                            effects.push(FileLockDialogEffect::FocusPanel);
                        }
                        FileLockAction::Prefill(path) => {
                            self.open = true;
                            self.apply_selected_path(path);
                            effects.push(FileLockDialogEffect::FocusPanel);
                        }
                        FileLockAction::ChooseFile => {
                            if !self.revalidate_root_pass(ctx, pending.provenance) {
                                continue;
                            }
                            if let Some(path) = self.pick_file()
                                && self.revalidate_root_pass(ctx, pending.provenance)
                            {
                                if self.apply_selected_path(path) {
                                    self.notice = None;
                                }
                            }
                        }
                        FileLockAction::Inspect(path) => {
                            if self.selected_path.as_ref() != Some(&path) {
                                continue;
                            }
                            self.submit_inspection(path);
                        }
                        FileLockAction::Refresh { generation, path } => {
                            if generation != self.session.view().generation()
                                || self.selected_path.as_ref() != Some(&path)
                            {
                                continue;
                            }
                            self.submit_inspection(path);
                        }
                        FileLockAction::Cancel { generation } => {
                            if generation == self.session.view().generation() {
                                match self.session.cancel() {
                                    Ok(_) => {
                                        self.invalidate_selection();
                                        self.cancel_reveal();
                                        self.notice = Some("Inspection cancelled.".into());
                                    }
                                    Err(error) => self.notice = Some(error.to_string()),
                                }
                            }
                        }
                        FileLockAction::SelectProcess {
                            generation,
                            process_id,
                            creation_filetime,
                        } => {
                            let key = (generation, process_id, creation_filetime);
                            if self.process_is_current(key) {
                                if self.selected_process != Some(key) {
                                    self.cancel_reveal();
                                }
                                self.selected_process = Some(key);
                            }
                        }
                        FileLockAction::CopySnapshot { generation } => {
                            if let Some(inspection) = self.current_inspection(&self.session.view())
                                && self
                                    .submitted
                                    .as_ref()
                                    .is_some_and(|submitted| submitted.generation == generation)
                            {
                                effects.push(FileLockDialogEffect::CopySnapshot(copy_snapshot(
                                    inspection,
                                )));
                            }
                        }
                        FileLockAction::Reveal(request) => {
                            if self.reveal_request_is_current(&request) {
                                self.start_reveal(request, ctx);
                            }
                        }
                        FileLockAction::Close => {
                            self.close_current_selection();
                        }
                    }
                }
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        }

        if self
            .reveal_ready
            .as_ref()
            .is_some_and(|ready| !self.reveal_request_is_current(&ready.request))
        {
            self.reveal_ready = None;
        }
        if self
            .reveal_ready
            .as_ref()
            .is_some_and(|ready| !ready.dispatched)
            && self.latest_pass.is_some_and(|pass| {
                pass.viewport_id == egui::ViewportId::ROOT && pass.was_retained(ctx)
            })
        {
            if let Some(ready) = self.reveal_ready.as_mut() {
                ready.dispatched = true;
                effects.push(FileLockDialogEffect::RevealExecutable(
                    ready.request.clone(),
                ));
            }
        } else if self.reveal_ready.is_some() {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        effects
    }

    pub(crate) fn reveal_request_is_current(&self, request: &FileLockRevealRequest) -> bool {
        self.submitted
            .as_ref()
            .is_some_and(|submitted| submitted.generation == request.generation)
            && self.selected_process
                == Some((
                    request.generation,
                    request.process_id,
                    request.creation_filetime,
                ))
            && self
                .current_inspection(&self.session.view())
                .is_some_and(|inspection| {
                    inspection.processes.iter().any(|process| {
                        process.process_id == request.process_id
                            && process.creation_filetime == request.creation_filetime
                            && process.executable_path.as_ref() == Some(&request.executable_path)
                    })
                })
    }

    pub(crate) fn reveal_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Requested executable reveal.".into(),
            Err(error) => format!("Could not reveal the executable: {error}"),
        });
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Inspection snapshot copied.".into(),
            Err(error) => format!("Could not copy the inspection snapshot: {error}"),
        });
    }

    pub(crate) fn take_reveal_path_for_dispatch(
        &mut self,
        request: &FileLockRevealRequest,
    ) -> Result<Option<PathBuf>, String> {
        if !self.reveal_request_is_current(request) {
            return Ok(None);
        }
        let Some(ready) = self.reveal_ready.as_ref() else {
            return Ok(None);
        };
        if !ready.dispatched || ready.request != *request {
            return Ok(None);
        }
        if let Some(executable) = ready.executable.as_ref() {
            if executable.path() != request.executable_path.as_path() {
                self.reveal_ready = None;
                return Err("the verified executable path changed before reveal".into());
            }
            if let Err(error) = executable.ensure_process_is_alive() {
                self.reveal_ready = None;
                return Err(error);
            }
        }
        let Some(ready) = self.reveal_ready.take() else {
            return Ok(None);
        };
        Ok(Some(
            ready
                .executable
                .map(|executable| executable.path().to_path_buf())
                .unwrap_or_else(|| ready.request.executable_path),
        ))
    }

    #[cfg(test)]
    pub(crate) fn set_file_choice_for_test(&mut self, choice: Option<PathBuf>) {
        self.test_file_choice = Some(choice);
    }

    #[cfg(test)]
    pub(crate) fn file_picker_calls_for_test(&self) -> usize {
        self.test_file_picker_calls
    }

    #[cfg(test)]
    pub(crate) fn set_reveal_result_for_test(&mut self, result: Result<PathBuf, String>) {
        self.test_reveal_result = Some(result);
    }

    #[cfg(test)]
    pub(crate) fn set_session_for_test(&mut self, session: FileLockSession) {
        self.session.close();
        self.session = session;
    }

    #[cfg(test)]
    pub(crate) fn selected_path_for_test(&self) -> Option<&std::path::Path> {
        self.selected_path.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn has_current_inspection_for_test(&self) -> bool {
        self.current_inspection(&self.session.view()).is_some()
    }

    #[cfg(test)]
    pub(crate) fn session_view_for_test(&self) -> crate::file_lock::FileLockSessionView {
        self.session.view()
    }

    #[cfg(test)]
    pub(crate) fn notice_for_test(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_action_rect(&self, name: &str) -> Option<(egui::Rect, egui::Rect)> {
        self.test_action_rects
            .iter()
            .find(|(candidate, _, _)| candidate == name)
            .map(|(_, rect, clip)| (*rect, *clip))
    }

    #[cfg(test)]
    fn record_action_rect(&mut self, name: &str, rect: egui::Rect, clip: egui::Rect) {
        self.test_action_rects.push((name.to_owned(), rect, clip));
    }

    #[cfg(not(test))]
    fn record_action_rect(&mut self, _name: &str, _rect: egui::Rect, _clip: egui::Rect) {}

    fn current_inspection<'a>(
        &self,
        view: &'a crate::file_lock::FileLockSessionView,
    ) -> Option<&'a FileLockInspection> {
        let submitted = self.submitted.as_ref()?;
        if submitted.generation != view.generation()
            || !view.inspection_is_current()
            || self.selected_path.as_ref() != Some(&submitted.path)
        {
            return None;
        }
        let inspection = view.inspection()?.as_ref();
        (inspection.path == submitted.path).then_some(inspection)
    }

    fn submitted_matches_view(&self, view: &crate::file_lock::FileLockSessionView) -> bool {
        self.submitted.as_ref().is_some_and(|submitted| {
            submitted.generation == view.generation()
                && self.selected_path.as_ref() == Some(&submitted.path)
        })
    }

    fn submit_inspection(&mut self, path: PathBuf) {
        self.invalidate_selection();
        self.cancel_reveal();
        if let Err(error) = crate::file_lock::validate_file_path(&path) {
            self.submitted = None;
            self.notice = Some(error.to_string());
            return;
        }
        match self.session.refresh(path.clone()) {
            Ok(generation) => {
                self.submitted = Some(SubmittedInspection { generation, path });
                self.notice = None;
            }
            Err(error) => {
                self.submitted = None;
                self.notice = Some(error.to_string());
            }
        }
    }

    fn apply_selected_path(&mut self, path: PathBuf) -> bool {
        if let Err(error) = crate::file_lock::validate_file_path(&path) {
            if self.selected_path.as_ref() != Some(&path) {
                self.cancel_live_inspection();
                self.selected_path = None;
                self.invalidate_selection();
                self.cancel_reveal();
            }
            self.notice = Some(error.to_string());
            return false;
        }
        if self.selected_path.as_ref() == Some(&path) {
            return true;
        }
        self.cancel_live_inspection();
        self.selected_path = Some(path);
        self.invalidate_selection();
        self.cancel_reveal();
        self.notice = None;
        true
    }

    fn invalidate_selection(&mut self) {
        self.submitted = None;
        self.selected_process = None;
        self.reveal_ready = None;
    }

    fn close_current_selection(&mut self) {
        self.cancel_live_inspection();
        self.invalidate_selection();
        self.cancel_reveal();
        self.notice = None;
        self.open = false;
    }

    fn cancel_live_inspection(&mut self) {
        if matches!(
            self.session.view().status(),
            FileLockSessionStatus::Inspecting | FileLockSessionStatus::RefreshQueued
        ) {
            let _ = self.session.cancel();
        }
    }

    fn process_is_current(&self, key: (u64, u32, u64)) -> bool {
        let (generation, process_id, creation_filetime) = key;
        self.submitted
            .as_ref()
            .is_some_and(|submitted| submitted.generation == generation)
            && self
                .current_inspection(&self.session.view())
                .is_some_and(|inspection| {
                    inspection.processes.iter().any(|process| {
                        process.process_id == process_id
                            && process.creation_filetime == creation_filetime
                    })
                })
    }

    fn start_reveal(&mut self, request: FileLockRevealRequest, ctx: &egui::Context) {
        if self.reveal_retirement_failed {
            self.notice = Some(
                "Executable validation could not be safely retired; this inspector will not start another reveal check.".into(),
            );
            return;
        }
        if self.reveal_worker.is_some() {
            self.notice = Some("An executable check is already in progress.".into());
            return;
        }
        #[cfg(test)]
        if let Some(result) = self.test_reveal_result.take() {
            match result {
                Ok(path) if path == request.executable_path => {
                    self.reveal_ready = Some(ReadyReveal {
                        request,
                        executable: None,
                        dispatched: false,
                    });
                }
                Ok(_) => self.notice = Some("The reported executable path changed.".into()),
                Err(error) => self.notice = Some(error),
            }
            return;
        }

        let permit = match crate::thread_reaper::reserve() {
            Ok(permit) => permit,
            Err(error) => {
                self.notice = Some(format!("Could not reserve executable validation: {error}"));
                return;
            }
        };
        let completion = permit.completion_notifier();
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let pid = request.process_id;
        let creation_filetime = request.creation_filetime;
        let expected_path = request.executable_path.clone();
        let join = thread::Builder::new()
            .name("file-lock-executable-check".into())
            .spawn(move || {
                let result = if worker_cancelled.load(Ordering::Acquire) {
                    Err(
                        "the selected process changed before its executable could be checked"
                            .into(),
                    )
                } else {
                    crate::file_lock::validate_reported_executable(
                        pid,
                        creation_filetime,
                        &expected_path,
                    )
                };
                if !worker_cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(result);
                }
                drop(sender);
                drop(completion);
            });
        match join {
            Ok(join) => {
                let joined = Arc::new(AtomicBool::new(false));
                let retirement_unavailable = Arc::new(AtomicBool::new(false));
                let receipt = RevealRetirementReceipt {
                    joined: Arc::clone(&joined),
                    unavailable: Arc::clone(&retirement_unavailable),
                    context: ctx.clone(),
                    resolved: false,
                };
                let reap_result = permit.reap_with_callback(join, move |_| receipt.joined());
                self.reveal_worker = Some(RevealWorker {
                    request,
                    cancelled,
                    result: receiver,
                    joined,
                    retirement_unavailable,
                });
                self.notice = Some(match reap_result {
                    Ok(()) => {
                        "Checking that the reported process and executable are still current…"
                            .into()
                    }
                    Err(error) => format!(
                        "Executable validation is running, but its worker could not be safely reaped: {error}"
                    ),
                });
            }
            Err(error) => {
                self.notice = Some(format!("Could not start executable validation: {error}"))
            }
        }
    }

    fn cancel_reveal(&mut self) {
        self.reveal_ready = None;
        if let Some(work) = self.reveal_worker.as_ref() {
            work.cancelled.store(true, Ordering::Release);
        }
    }

    fn poll_reveal_work(&mut self) {
        let Some(work) = self.reveal_worker.as_ref() else {
            return;
        };
        if work.retirement_unavailable.load(Ordering::Acquire) {
            if let Some(work) = self.reveal_worker.take() {
                work.cancelled.store(true, Ordering::Release);
            }
            self.reveal_retirement_failed = true;
            self.notice = Some(
                "Executable validation could not be safely retired; this inspector will not start another reveal check.".into(),
            );
            return;
        }
        if !work.joined.load(Ordering::Acquire) {
            return;
        }
        let Some(work) = self.reveal_worker.take() else {
            return;
        };
        if work.cancelled.load(Ordering::Acquire) {
            return;
        }
        match work.result.try_recv() {
            Ok(Ok(executable)) if executable.path() == work.request.executable_path => {
                self.reveal_ready = Some(ReadyReveal {
                    request: work.request,
                    executable: Some(executable),
                    dispatched: false,
                });
                self.notice = None;
            }
            Ok(Ok(_)) => {
                self.notice = Some("The reported executable path changed.".into());
            }
            Ok(Err(error)) => self.notice = Some(error),
            Err(_) => self.notice = Some("Executable validation did not return a result.".into()),
        }
    }

    fn stage(&mut self, ctx: &egui::Context, actions: Vec<FileLockAction>) {
        self.stage_with_provenance(
            ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
            actions,
        );
    }

    fn stage_with_provenance(
        &mut self,
        provenance: ViewportPassProvenance,
        actions: Vec<FileLockAction>,
    ) {
        if actions.is_empty() {
            return;
        }
        if actions
            .iter()
            .any(|action| matches!(action, FileLockAction::Close))
        {
            self.pending = Some(PendingFileLockPass {
                provenance,
                actions: vec![FileLockAction::Close],
            });
            return;
        }
        match self.pending.as_mut() {
            Some(pending) if pending.provenance == provenance => {
                if !pending
                    .actions
                    .iter()
                    .any(|action| matches!(action, FileLockAction::Close))
                {
                    pending.actions.extend(actions);
                }
            }
            _ => {
                self.pending = Some(PendingFileLockPass {
                    provenance,
                    actions,
                });
            }
        }
    }

    fn revalidate_root_pass(
        &self,
        ctx: &egui::Context,
        provenance: ViewportPassProvenance,
    ) -> bool {
        provenance.viewport_id == egui::ViewportId::ROOT && provenance.was_retained(ctx)
    }

    fn pick_file(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(choice) = self.test_file_choice.take() {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return choice;
        }

        #[cfg(not(test))]
        {
            rfd::FileDialog::new().pick_file()
        }
        #[cfg(test)]
        {
            None
        }
    }
}

fn show_inspection_status(
    ui: &mut egui::Ui,
    view: Option<&crate::file_lock::FileLockSessionView>,
    inspection: Option<&FileLockInspection>,
) {
    if let Some(inspection) = inspection {
        let inspected_at = chrono::DateTime::<chrono::Local>::from(inspection.inspected_at);
        ui.label(format!(
            "Windows inspection completed {}",
            inspected_at.format("%Y-%m-%d %H:%M:%S %Z"),
        ));
        return;
    }
    let Some(view) = view else {
        return;
    };
    match view.status() {
        FileLockSessionStatus::Idle => {}
        FileLockSessionStatus::Inspecting => {
            ui.label("Inspecting reported file users…");
        }
        FileLockSessionStatus::RefreshQueued => {
            ui.label("A newer inspection is queued…");
        }
        FileLockSessionStatus::Complete => {
            ui.label("The completed inspection is no longer current; inspect again.");
        }
        FileLockSessionStatus::Failed => {
            if let Some(error) = view.inspection_error() {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            } else if let Some(error) = view.worker_error() {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            }
        }
        FileLockSessionStatus::Cancelled => {
            ui.label("Inspection cancelled.");
        }
        FileLockSessionStatus::Closed => {
            ui.label("The inspection session is closed.");
        }
    };
}

fn process_details_unavailable(process: &FileLockProcess) -> bool {
    process.executable_path.is_none()
        || (process.application_name.is_empty() && process.service_name.is_empty())
}

#[cfg(test)]
pub(crate) fn process_details_unavailable_for_test(process: &FileLockProcess) -> bool {
    process_details_unavailable(process)
}

fn process_label(process: &FileLockProcess) -> String {
    if !process.application_name.is_empty() {
        process.application_name.clone()
    } else if !process.service_name.is_empty() {
        process.service_name.clone()
    } else {
        "Process details unavailable".into()
    }
}

fn format_process_start(filetime: u64) -> String {
    const WINDOWS_TO_UNIX_EPOCH_SECONDS: i64 = 11_644_473_600;
    let seconds = (filetime / 10_000_000) as i64 - WINDOWS_TO_UNIX_EPOCH_SECONDS;
    let nanoseconds = ((filetime % 10_000_000) * 100) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanoseconds)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string()
        })
        .unwrap_or_else(|| "time unavailable".into())
}

fn copy_snapshot(inspection: &FileLockInspection) -> String {
    let inspected_at = chrono::DateTime::<chrono::Local>::from(inspection.inspected_at);
    let mut output = format!(
        "Windows reported users for {}\nInspected {}\n",
        inspection.path.display(),
        inspected_at.format("%Y-%m-%d %H:%M:%S %Z"),
    );
    for process in &inspection.processes {
        use std::fmt::Write as _;
        let _ = writeln!(
            output,
            "PID {}\tFILETIME {}\t{}\t{}\t{}",
            process.process_id,
            process.creation_filetime,
            process.application_name,
            process.service_name,
            process
                .executable_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "executable unavailable".into()),
        );
    }
    if inspection.processes.is_empty() {
        output.push_str("No processes reported.\n");
    }
    output.push_str(
        "These are processes Windows reported; this list does not prove which process is denying access.\n",
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_lock_ui_stale_ready_reveal_is_retired_and_failed_check_is_not_actionable() {
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().expect("isolated lexical path root");
        let selected_path = directory.path().join("selected.txt");
        let changed_path = directory.path().join("changed.txt");
        let executable_path = directory.path().join("reported.exe");
        let job_executable = executable_path.clone();
        let session = FileLockSession::with_test_job(
            move |path| {
                Ok(FileLockInspection {
                    path,
                    identity: crate::file_operations::NativeFileIdentity::from_parts(1, [4; 16]),
                    inspected_at: std::time::SystemTime::now(),
                    processes: vec![FileLockProcess {
                        process_id: 57,
                        creation_filetime: 1_330_000_000_000_000,
                        application_name: "Fixture process".into(),
                        service_name: String::new(),
                        executable_path: Some(job_executable.clone()),
                    }],
                })
            },
            || {},
        );
        let mut state = FileLockDialogState::new(&ctx);
        state.set_session_for_test(session);
        state.selected_path = Some(selected_path.clone());
        state.submit_inspection(selected_path.clone());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !state.has_current_inspection_for_test() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(state.has_current_inspection_for_test());

        let key = (
            state.submitted.as_ref().unwrap().generation,
            57,
            1_330_000_000_000_000,
        );
        state.selected_process = Some(key);
        let request = FileLockRevealRequest {
            generation: key.0,
            process_id: key.1,
            creation_filetime: key.2,
            executable_path: executable_path.clone(),
        };

        state.test_reveal_result = Some(Err("the process identity changed".into()));
        state.start_reveal(request.clone(), &ctx);
        assert!(state.reveal_ready.is_none());
        assert_eq!(
            state.notice_for_test(),
            Some("the process identity changed")
        );

        state.test_reveal_result = Some(Ok(executable_path));
        state.start_reveal(request.clone(), &ctx);
        assert!(state.reveal_ready.is_some());
        assert!(state.reveal_request_is_current(&request));
        state.reveal_ready.as_mut().unwrap().dispatched = true;

        assert!(state.apply_selected_path(changed_path));
        assert!(state.reveal_ready.is_none());
        assert!(!state.reveal_request_is_current(&request));
        assert!(
            state
                .take_reveal_path_for_dispatch(&request)
                .expect("a stale request is ignored")
                .is_none()
        );
        assert!(state.on_ui_frame_complete(&ctx).is_empty());
        state.shutdown();
    }
}
