//! Completion-bearing adapters for Hot Folder launcher and MkMacro actions.
//!
//! Launcher commands cross a bounded queue to the existing GUI-owned typed
//! CommandBus. Macro runs use the existing worker admission and terminal
//! publication path, with cancellation tied to the admitted run ID.

use crate::commands::{
    ActivationSource, Command, CommandError, CommandInvocation, CommandOutcome, ExternalCommand,
    parse_action,
};
use crate::hotfolder::{
    HotFolderExternalActionAdapter, HotFolderExternalActionOutcome,
    HotFolderExternalActionRequest, HotFolderExecutionPhase, HotFolderTerminalStatus,
    RuleAction, bind_mkmacro_action_with_dependencies, bind_registered_launcher_action,
};
use crate::mkmacro::{MkMacroStore, RuntimeState};
use crate::universal_actions::{
    ActionSurface, PersistedUniversalActionRef, ResolvedActionTarget, UniversalActionOperation,
    UniversalActionRegistry, action_ids,
};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::time::Duration;

const LAUNCHER_QUEUE_CAPACITY: usize = 16;
const MAX_LAUNCHER_DISPATCHES_PER_FRAME: usize = 8;
const LAUNCHER_WAIT_SLICE: Duration = Duration::from_millis(20);

const REQUEST_PENDING: u8 = 0;
const REQUEST_STARTED: u8 = 1;
const REQUEST_CANCELLED: u8 = 2;

struct PendingLauncherAction {
    request: HotFolderExternalActionRequest,
    state: Arc<AtomicU8>,
    reply: mpsc::SyncSender<HotFolderExternalActionOutcome>,
}

pub(crate) struct LauncherPendingReply {
    operation_id: u128,
    state: Arc<AtomicU8>,
    response: Receiver<HotFolderExternalActionOutcome>,
}

impl LauncherPendingReply {
    pub(crate) fn cancel_if_pending(&self) -> bool {
        self.state
            .compare_exchange(
                REQUEST_PENDING,
                REQUEST_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(crate) fn receive_timeout(
        &self,
        timeout: Duration,
    ) -> Result<HotFolderExternalActionOutcome, mpsc::RecvTimeoutError> {
        let outcome = self.response.recv_timeout(timeout)?;
        if outcome.operation_id == self.operation_id {
            Ok(outcome)
        } else {
            Err(mpsc::RecvTimeoutError::Disconnected)
        }
    }
}

pub(crate) struct LauncherHotFolderActionAdapter {
    launcher_tx: SyncSender<PendingLauncherAction>,
    launcher_active: Arc<AtomicBool>,
    repaint: egui::Context,
    mkmacro_store: Arc<MkMacroStore>,
}

pub(crate) struct LauncherActionCompletionPump {
    launcher_rx: Receiver<PendingLauncherAction>,
    launcher_active: Arc<AtomicBool>,
}

impl LauncherHotFolderActionAdapter {
    pub(crate) fn new(
        repaint: egui::Context,
        mkmacro_store: Arc<MkMacroStore>,
    ) -> (Arc<Self>, LauncherActionCompletionPump) {
        let (launcher_tx, launcher_rx) = mpsc::sync_channel(LAUNCHER_QUEUE_CAPACITY);
        let launcher_active = Arc::new(AtomicBool::new(true));
        (
            Arc::new(Self {
                launcher_tx,
                launcher_active: launcher_active.clone(),
                repaint,
                mkmacro_store,
            }),
            LauncherActionCompletionPump {
                launcher_rx,
                launcher_active,
            },
        )
    }

    pub(crate) fn set_active(&self, active: bool) {
        self.launcher_active.store(active, Ordering::Release);
    }

    fn dispatch_launcher(
        &self,
        request: HotFolderExternalActionRequest,
        cancellation: &crate::file_operations::ScanCancellation,
    ) -> HotFolderExternalActionOutcome {
        let pending = match self.enqueue_launcher_request(request.clone()) {
            Ok(pending) => pending,
            Err(outcome) => return outcome,
        };

        loop {
            if cancellation.is_cancelled()
                && pending.cancel_if_pending()
            {
                return cancelled(request.operation_id, None);
            }

            match pending.receive_timeout(LAUNCHER_WAIT_SLICE) {
                Ok(outcome) if outcome.operation_id == request.operation_id => return outcome,
                Ok(_) => {
                    return failure(
                        request.operation_id,
                        "launcher returned a mismatched completion",
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if pending.state.load(Ordering::Acquire) == REQUEST_CANCELLED {
                        return cancelled(request.operation_id, None);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return failure(
                        request.operation_id,
                        "launcher completion receipt was disconnected",
                    );
                }
            }
        }
    }

    pub(crate) fn enqueue_launcher_request(
        &self,
        request: HotFolderExternalActionRequest,
    ) -> Result<LauncherPendingReply, HotFolderExternalActionOutcome> {
        if !self.launcher_active.load(Ordering::Acquire) {
            return Err(failure(
                request.operation_id,
                "launcher completion bridge is disconnected",
            ));
        }
        let state = Arc::new(AtomicU8::new(REQUEST_PENDING));
        let (reply, response) = mpsc::sync_channel(1);
        let pending = PendingLauncherAction {
            request: request.clone(),
            state: state.clone(),
            reply,
        };
        match self.launcher_tx.try_send(pending) {
            Ok(()) => {
                self.repaint.request_repaint();
                Ok(LauncherPendingReply {
                    operation_id: request.operation_id,
                    state,
                    response,
                })
            }
            Err(TrySendError::Full(_)) => {
                Err(failure(request.operation_id, "launcher dispatch queue is full"))
            }
            Err(TrySendError::Disconnected(_)) => Err(failure(
                request.operation_id,
                "launcher completion bridge is disconnected",
            )),
        }
    }

    fn dispatch_mkmacro(
        &self,
        request: HotFolderExternalActionRequest,
        cancellation: &crate::file_operations::ScanCancellation,
    ) -> HotFolderExternalActionOutcome {
        let RuleAction::MkMacro {
            macro_id,
            configuration_fingerprint: _,
        } = &request.action
        else {
            return failure(request.operation_id, "MkMacro target is invalid");
        };
        if cancellation.is_cancelled() {
            return cancelled(request.operation_id, None);
        }
        let document = self.mkmacro_store.snapshot();
        let mut matching = document.macros.iter().filter(|macro_| macro_.id == *macro_id);
        let Some(current) = matching.next() else {
            return failure(request.operation_id, "MkMacro target is unavailable");
        };
        if matching.next().is_some() {
            return failure(request.operation_id, "MkMacro configuration changed");
        }
        let (bound_action, expected_dependencies) =
            match bind_mkmacro_action_with_dependencies(&document, *macro_id) {
                Ok(binding) => binding,
                Err(_) => {
                    return failure(request.operation_id, "MkMacro configuration changed");
                }
            };
        if bound_action != request.action {
            return failure(request.operation_id, "MkMacro configuration changed");
        }

        let run =
            match crate::mkmacro::runtime::start_hotfolder_macro(current, &expected_dependencies) {
            Ok(run) => run,
            Err(diagnostic) => {
                let reason = match diagnostic.kind {
                    crate::mkmacro::DiagnosticKind::RuntimeUnavailable => {
                        "MkMacro runtime is unavailable or busy"
                    }
                    crate::mkmacro::DiagnosticKind::InvalidTarget => {
                        "MkMacro configuration changed or invocation was rejected"
                    }
                    _ => "MkMacro invocation was rejected",
                };
                return failure(request.operation_id, reason);
            }
        };
        let run_id = run.run_id();
        let mut cancellation_requested = !run.activate_if_not_cancelled(cancellation);
        loop {
            if cancellation.is_cancelled() && !cancellation_requested {
                cancellation_requested = true;
                // The runtime atomically matches the owned ID and latches Stop.
                // It may return false if this run already completed; in either
                // case keep the handle until its actual terminal receipt arrives.
                run.stop_if_current();
            }

            match run.wait_completion(LAUNCHER_WAIT_SLICE) {
                Ok(completion) if completion.run_id == run_id => {
                    let (status, reason) = match completion.state {
                        RuntimeState::Completed => (HotFolderTerminalStatus::Success, None),
                        RuntimeState::Stopped => (
                            HotFolderTerminalStatus::Cancelled,
                            Some("MkMacro run was cancelled".to_owned()),
                        ),
                        RuntimeState::Failed => (
                            HotFolderTerminalStatus::Failure,
                            Some(format!(
                                "MkMacro run failed ({:?})",
                                completion.failure_kind.unwrap_or(
                                    crate::mkmacro::DiagnosticKind::RuntimeUnavailable
                                )
                            )),
                        ),
                        _ => (
                            HotFolderTerminalStatus::Failure,
                            Some("MkMacro returned a nonterminal completion".to_owned()),
                        ),
                    };
                    return HotFolderExternalActionOutcome {
                        operation_id: request.operation_id,
                        macro_run_id: Some(run_id),
                        status,
                        phase: HotFolderExecutionPhase::Complete,
                        reason,
                    };
                }
                Ok(_) => {
                    return HotFolderExternalActionOutcome {
                        operation_id: request.operation_id,
                        macro_run_id: Some(run_id),
                        status: HotFolderTerminalStatus::Failure,
                        phase: HotFolderExecutionPhase::Complete,
                        reason: Some("MkMacro returned a mismatched terminal receipt".into()),
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return HotFolderExternalActionOutcome {
                        operation_id: request.operation_id,
                        macro_run_id: Some(run_id),
                        status: HotFolderTerminalStatus::Failure,
                        phase: HotFolderExecutionPhase::Complete,
                        reason: Some("MkMacro worker stopped before its terminal receipt".into()),
                    };
                }
            }
        }
    }
}

impl HotFolderExternalActionAdapter for LauncherHotFolderActionAdapter {
    fn supports(&self, action: &RuleAction) -> bool {
        match action {
            RuleAction::RegisteredLauncherAction {
                reference,
                configuration_fingerprint,
            } => {
                self.launcher_active.load(Ordering::Acquire)
                    && reference.action_id == action_ids::RESULT_EXECUTE
                    && reference.target.is_some()
                    && configuration_fingerprint.is_valid()
            }
            RuleAction::MkMacro { .. } => {
                self.launcher_active.load(Ordering::Acquire)
                    && crate::mkmacro::runtime::hotfolder_runtime_available()
            }
            RuleAction::Copy { .. } | RuleAction::Move { .. } | RuleAction::Rename { .. } => false,
        }
    }

    fn dispatch(
        &self,
        request: HotFolderExternalActionRequest,
        cancellation: &crate::file_operations::ScanCancellation,
    ) -> HotFolderExternalActionOutcome {
        if !self.launcher_active.load(Ordering::Acquire) {
            return failure(request.operation_id, "Hot Folder runtime is disabled");
        }
        if request.operation_id == 0
            || request.authorization.epoch == 0
            || request.authorization.rule_fingerprint != request.rule_fingerprint
            || request.candidate_id.get() == 0
        {
            return failure(request.operation_id, "Hot Folder authorization evidence is invalid");
        }
        match request.action.clone() {
            RuleAction::RegisteredLauncherAction { .. } => {
                if !self.supports(&request.action) {
                    return failure(request.operation_id, "registered launcher action is unsupported");
                }
                if request.authorization.enablement
                    != crate::hotfolder::ActionEnablement::StrongConfirmationRequired
                {
                    return failure(
                        request.operation_id,
                        "launcher action lacks strong enable authorization",
                    );
                }
                self.dispatch_launcher(request, cancellation)
            }
            RuleAction::MkMacro { .. } => self.dispatch_mkmacro(request, cancellation),
            RuleAction::Copy { .. } | RuleAction::Move { .. } | RuleAction::Rename { .. } => {
                failure(request.operation_id, "external adapter received a built-in action")
            }
        }
    }
}

impl super::LauncherApp {
    pub(crate) fn hotfolder_external_action_adapter(
        &self,
    ) -> Arc<dyn HotFolderExternalActionAdapter> {
        self.hotfolder_action_adapter.clone()
    }
}

impl LauncherActionCompletionPump {
    fn take_pending(&self, maximum: usize) -> Vec<PendingLauncherAction> {
        let mut pending = Vec::with_capacity(maximum.min(LAUNCHER_QUEUE_CAPACITY));
        while pending.len() < maximum {
            match self.launcher_rx.try_recv() {
                Ok(request) => pending.push(request),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        pending
    }

    pub(crate) fn set_active(&self, active: bool) {
        self.launcher_active.store(active, Ordering::Release);
        if !active {
            self.retire_pending();
        }
    }

    pub(crate) fn retire_pending(&self) {
        while let Ok(pending) = self.launcher_rx.try_recv() {
            pending.state.store(REQUEST_CANCELLED, Ordering::Release);
            let _ = pending.reply.try_send(failure(
                pending.request.operation_id,
                "Hot Folder launcher dispatch was retired",
            ));
        }
    }
}

impl Drop for LauncherActionCompletionPump {
    fn drop(&mut self) {
        self.launcher_active.store(false, Ordering::Release);
        self.retire_pending();
    }
}

pub(crate) fn pump_launcher_completions(app: &mut super::LauncherApp) {
    if !app
        .hotfolder_action_pump
        .launcher_active
        .load(Ordering::Acquire)
    {
        app.hotfolder_action_pump.retire_pending();
        return;
    }
    let pending = app
        .hotfolder_action_pump
        .take_pending(MAX_LAUNCHER_DISPATCHES_PER_FRAME);
    complete_launcher_actions(pending, |request| {
        invoke_current_registered_action(app, request)
    });
}

pub(crate) fn pump_launcher_completions_with(
    pump: &LauncherActionCompletionPump,
    invoke: impl FnMut(&HotFolderExternalActionRequest) -> HotFolderExternalActionOutcome,
) {
    let pending = pump.take_pending(MAX_LAUNCHER_DISPATCHES_PER_FRAME);
    complete_launcher_actions(pending, invoke);
}

fn complete_launcher_actions(
    pending: Vec<PendingLauncherAction>,
    mut invoke: impl FnMut(&HotFolderExternalActionRequest) -> HotFolderExternalActionOutcome,
) {
    for pending in pending {
        if pending
            .state
            .compare_exchange(
                REQUEST_PENDING,
                REQUEST_STARTED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            let _ = pending.reply.try_send(cancelled(pending.request.operation_id, None));
            continue;
        }

        let outcome = invoke(&pending.request);
        pending.state.store(REQUEST_STARTED, Ordering::Release);
        let _ = pending.reply.try_send(outcome);
    }
}

fn invoke_current_registered_action(
    app: &mut super::LauncherApp,
    request: &HotFolderExternalActionRequest,
) -> HotFolderExternalActionOutcome {
    let snapshot = app.universal_action_catalog_snapshot();
    let command_bus = app.command_bus.clone();
    invoke_registered_action_from_catalog(request, &snapshot.entries, |invocation| {
        command_bus.dispatch(invocation, app)
    })
}

pub(crate) fn invoke_registered_action_from_catalog(
    request: &HotFolderExternalActionRequest,
    entries: &[ResolvedActionTarget],
    dispatch: impl FnOnce(
        &CommandInvocation,
    ) -> Result<CommandOutcome, CommandError>,
) -> HotFolderExternalActionOutcome {
    let RuleAction::RegisteredLauncherAction {
        reference,
        configuration_fingerprint,
    } = &request.action
    else {
        return failure(request.operation_id, "registered launcher target is invalid");
    };
    let Some(saved_target) = reference.target.as_ref() else {
        return failure(request.operation_id, "registered launcher target has no stable identity");
    };

    let mut matching = entries
        .iter()
        .filter(|entry| entry.target.persistent_ref().as_ref() == Some(saved_target));
    let Some(target) = matching.next() else {
        return failure(request.operation_id, "registered launcher target is unavailable");
    };
    if matching.next().is_some()
        || bind_registered_launcher_action(target).as_ref() != Ok(&request.action)
    {
        return failure(request.operation_id, "registered launcher configuration changed");
    }

    let resolution = super::universal_action_catalog::action_resolution_context_for_target(
        &target.target,
        ActionSurface::LauncherList,
        "",
    );
    if reference.action_id != action_ids::RESULT_EXECUTE
        || !configuration_fingerprint.is_valid()
    {
        return failure(request.operation_id, "registered launcher action is unsupported");
    }
    let mut registered = UniversalActionRegistry
        .resolve(target, &resolution)
        .into_iter()
        .filter(|action| action.id == reference.action_id);
    let Some(registered) = registered.next() else {
        return failure(request.operation_id, "registered launcher action is unavailable");
    };
    if registered.is_available() == false || registered.id != action_ids::RESULT_EXECUTE {
        return failure(request.operation_id, "registered launcher action is disabled");
    }
    if registered.operation
        != UniversalActionOperation::InvokePrimary(target.selected_action.clone())
    {
        return failure(request.operation_id, "registered launcher action changed");
    }
    let action = match &registered.operation {
        UniversalActionOperation::InvokePrimary(action) => action,
        UniversalActionOperation::Command { .. } | UniversalActionOperation::UiIntent(_) => {
            return failure(request.operation_id, "registered launcher action needs a UI path");
        }
    };
    let command = match parse_action(action) {
        Ok(Command::External(command)) if registered_process_target(&command) => command,
        _ => return failure(request.operation_id, "registered launcher action is unsupported"),
    };
    if command.namespace.is_some() {
        return failure(request.operation_id, "registered launcher namespace is unsupported");
    }

    let invocation = CommandInvocation {
        command: Command::External(command),
        original_action: action.clone(),
        query_override: None,
        source: ActivationSource::HotFolder,
    };
    match dispatch(&invocation) {
        Ok(_outcome) => HotFolderExternalActionOutcome {
            operation_id: request.operation_id,
            macro_run_id: None,
            status: HotFolderTerminalStatus::Success,
            phase: HotFolderExecutionPhase::Complete,
            reason: None,
        },
        Err(_) => failure(request.operation_id, "registered launcher command failed"),
    }
}

fn registered_process_target(command: &ExternalCommand) -> bool {
    crate::hotfolder::is_registered_process_target(command)
}

fn cancelled(
    operation_id: u128,
    macro_run_id: Option<u64>,
) -> HotFolderExternalActionOutcome {
    HotFolderExternalActionOutcome {
        operation_id,
        macro_run_id,
        status: HotFolderTerminalStatus::Cancelled,
        phase: HotFolderExecutionPhase::Admission,
        reason: Some("external action was cancelled before completion".into()),
    }
}

fn failure(operation_id: u128, reason: &'static str) -> HotFolderExternalActionOutcome {
    HotFolderExternalActionOutcome {
        operation_id,
        macro_run_id: None,
        status: HotFolderTerminalStatus::Failure,
        phase: HotFolderExecutionPhase::Admission,
        reason: Some(reason.into()),
    }
}

