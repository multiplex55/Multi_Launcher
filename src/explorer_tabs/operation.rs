use std::num::NonZeroU64;

use super::identity::{ShellBindingIdentity, SnapshotFailure, WindowIdentity};

pub const MAX_FOLDER_PATH_UTF16_UNITS: usize = 32_767;
pub const MAX_REQUEST_FOLDERS: usize = 32;
pub const MAX_REQUEST_PATH_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderPath(Box<str>);

impl FolderPath {
    pub fn parse(value: &str) -> Result<Self, FolderPathError> {
        if value.is_empty() {
            return Err(FolderPathError::Empty);
        }
        if value
            .encode_utf16()
            .take(MAX_FOLDER_PATH_UTF16_UNITS + 1)
            .count()
            > MAX_FOLDER_PATH_UTF16_UNITS
        {
            return Err(FolderPathError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(FolderPathError::ControlCharacter);
        }
        if value.starts_with("\\\\?\\")
            || value.starts_with("\\\\.\\")
            || value.starts_with("\\??\\")
            || value.starts_with("\\\\??\\")
        {
            return Err(FolderPathError::DeviceNamespace);
        }
        if value
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(FolderPathError::InvalidCharacter);
        }

        let bytes = value.as_bytes();
        let drive_absolute = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/');

        if drive_absolute {
            if value[3..].contains(':') {
                return Err(FolderPathError::InvalidCharacter);
            }
            return Ok(Self(value.into()));
        }

        if let Some(rest) = value.strip_prefix("\\\\") {
            let mut components = rest.split('\\');
            let server = components.next().unwrap_or_default();
            let share = components.next().unwrap_or_default();
            if server.is_empty()
                || share.is_empty()
                || server == "."
                || server == ".."
                || share == "."
                || share == ".."
                || server.contains('/')
                || share.contains('/')
                || value[2..].contains(':')
            {
                return Err(FolderPathError::InvalidUncPath);
            }
            return Ok(Self(value.into()));
        }

        if bytes.len() >= 2 && bytes[1] == b':' {
            return Err(FolderPathError::RelativeDrivePath);
        }
        Err(FolderPathError::NotAbsolute)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn utf8_len(&self) -> usize {
        self.0.len()
    }
}

impl AsRef<str> for FolderPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderPathError {
    Empty,
    TooLong,
    ControlCharacter,
    DeviceNamespace,
    InvalidCharacter,
    InvalidUncPath,
    RelativeDrivePath,
    NotAbsolute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTabRequest {
    folders: Vec<FolderPath>,
}

impl FolderTabRequest {
    pub fn new(folders: Vec<FolderPath>) -> Result<Self, FolderRequestError> {
        if folders.is_empty() {
            return Err(FolderRequestError::Empty);
        }
        if folders.len() > MAX_REQUEST_FOLDERS {
            return Err(FolderRequestError::TooManyFolders);
        }
        let bytes = folders.iter().fold(0usize, |total, folder| {
            total.saturating_add(folder.utf8_len())
        });
        if bytes > MAX_REQUEST_PATH_BYTES {
            return Err(FolderRequestError::PathBytesExceeded);
        }
        Ok(Self { folders })
    }

    pub fn folders(&self) -> &[FolderPath] {
        &self.folders
    }

    pub fn len(&self) -> usize {
        self.folders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRequestError {
    Empty,
    TooManyFolders,
    PathBytesExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(NonZeroU64);

impl OperationId {
    /// The future worker supplies an invocation token when it creates the
    /// operation. It is intentionally distinct from that worker's lifecycle
    /// generation.
    pub(crate) fn from_token(token: u64) -> Option<Self> {
        NonZeroU64::new(token).map(Self)
    }
}

/// Stable attribution for one correlated Explorer window created for an
/// operation. This is distinct from a Shell registration identity, which may
/// differ between tabs hosted by the same window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TargetAuthorityId(NonZeroU64);

impl TargetAuthorityId {
    /// A2 assigns this opaque token only after correlating a newly created
    /// window with this operation's launch event.
    pub(crate) fn from_correlated_worker_token(token: u64) -> Option<Self> {
        NonZeroU64::new(token).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationControl {
    Continue,
    Cancelled,
    DeadlineExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRequestKind {
    /// The first requested path belongs in a newly correlated window's first
    /// tab. There is no arbitrary pre-existing tab to preserve as a bootstrap.
    CreateInitialWindow,
    /// Later requested paths append to the already-bound Explorer target.
    OpenAdditionalTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRequestStep {
    operation_id: OperationId,
    request_index: usize,
    expected_ordinal: u16,
    kind: FolderRequestKind,
    folder: FolderPath,
}

impl FolderRequestStep {
    pub fn request_index(&self) -> usize {
        self.request_index
    }

    pub fn expected_ordinal(&self) -> u16 {
        self.expected_ordinal
    }

    pub fn kind(&self) -> FolderRequestKind {
        self.kind
    }

    pub fn folder(&self) -> &FolderPath {
        &self.folder
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestResult {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderIdentityResult {
    ExactPidl,
    DifferentPidl,
    Unavailable,
}

/// Exact folder evidence is tied to the request slot and the original path.
/// This prevents an exact comparison made for another duplicate path or step
/// from satisfying the current operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderIdentityEvidence {
    request_index: usize,
    requested_path: FolderPath,
    result: FolderIdentityResult,
}

impl FolderIdentityEvidence {
    pub fn request_index(&self) -> usize {
        self.request_index
    }

    pub fn requested_path(&self) -> &FolderPath {
        &self.requested_path
    }

    pub fn result(&self) -> FolderIdentityResult {
        self.result
    }

    /// Called by the later correlation adapter only after it has compared
    /// the requested and observed folder PIDLs for this exact request step.
    pub(crate) fn from_pidl_check(
        request_index: usize,
        requested_path: FolderPath,
        result: FolderIdentityResult,
    ) -> Self {
        Self {
            request_index,
            requested_path,
            result,
        }
    }
}

/// Positive target attribution is supplied by the later launch-correlating
/// layer. A read-only Shell candidate cannot be converted into this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundTargetEvidence {
    operation_id: OperationId,
    authority_id: TargetAuthorityId,
    window: WindowIdentity,
}

impl BoundTargetEvidence {
    pub fn window(&self) -> WindowIdentity {
        self.window
    }

    pub fn authority_id(&self) -> TargetAuthorityId {
        self.authority_id
    }

    /// The caller creates this only after correlating an observed target with
    /// this operation's launch event. Shell-only path matches are insufficient.
    pub(crate) fn from_correlated_launch(
        operation_id: OperationId,
        authority_id: TargetAuthorityId,
        window: WindowIdentity,
    ) -> Self {
        Self {
            operation_id,
            authority_id,
            window,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialTargetEvidence {
    bound_target: BoundTargetEvidence,
    shell_binding: ShellBindingIdentity,
    tab_identity: VerifiedTabIdentity,
    ordinal: u16,
    folder: FolderIdentityEvidence,
}

impl InitialTargetEvidence {
    /// The A2 verifier creates this only after the correlated target's first
    /// visible tab has been identified and its PIDL checked against the step.
    pub(crate) fn verified(
        bound_target: BoundTargetEvidence,
        shell_binding: ShellBindingIdentity,
        tab_identity: VerifiedTabIdentity,
        ordinal: u16,
        folder: FolderIdentityEvidence,
    ) -> Self {
        Self {
            bound_target,
            shell_binding,
            tab_identity,
            ordinal,
            folder,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedTabEvidence {
    bound_target: BoundTargetEvidence,
    shell_binding: ShellBindingIdentity,
    identity: VerifiedTabIdentity,
    ordinal: u16,
    folder: FolderIdentityEvidence,
}

impl VerifiedTabEvidence {
    /// The A2 verifier creates this only from fresh target + UI Automation tab
    /// evidence, then binds the exact Shell PIDL comparison to this step.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn verified(
        bound_target: BoundTargetEvidence,
        shell_binding: ShellBindingIdentity,
        identity: VerifiedTabIdentity,
        ordinal: u16,
        folder: FolderIdentityEvidence,
    ) -> Self {
        Self {
            bound_target,
            shell_binding,
            identity,
            ordinal,
            folder,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VerifiedTabIdentity(NonZeroU64);

impl VerifiedTabIdentity {
    /// A2 assigns a worker-local monotonic token after UI Automation has
    /// established a distinct tab element. This is not an HWND or COM pointer.
    pub(crate) fn from_worker_token(token: u64) -> Option<Self> {
        NonZeroU64::new(token).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFolderTab {
    request_index: usize,
    folder: FolderPath,
    shell_binding: ShellBindingIdentity,
    identity: VerifiedTabIdentity,
    ordinal: u16,
}

impl VerifiedFolderTab {
    pub fn request_index(&self) -> usize {
        self.request_index
    }

    pub fn folder(&self) -> &FolderPath {
        &self.folder
    }

    pub fn shell_binding(&self) -> ShellBindingIdentity {
        self.shell_binding
    }

    pub fn identity(&self) -> VerifiedTabIdentity {
        self.identity
    }

    pub fn ordinal(&self) -> u16 {
        self.ordinal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationFailure {
    SnapshotIncomplete(SnapshotFailure),
    TargetChanged,
    WrongTargetEvidence,
    WrongRequestEvidence,
    UnexpectedTabOrdinal,
    ReusedTabIdentity,
    ShellBindingChanged,
    WrongFolder,
    FolderIdentityUnavailable,
    RequestRejected,
    StaleRequestResult,
    UnexpectedObservation,
    FinalVerificationFailed,
    CapabilityUnavailable,
    NativeOperationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    Running,
    Completed,
    Cancelled,
    DeadlineExpired,
    Failed(OperationFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationReport {
    verified_prefix: Vec<VerifiedFolderTab>,
    attempted_unverified: Option<FolderRequestStep>,
    remaining: Vec<FolderPath>,
    outcome: OperationOutcome,
}

impl OperationReport {
    pub fn verified_prefix(&self) -> &[VerifiedFolderTab] {
        &self.verified_prefix
    }

    /// Work whose native call may have run but lacks exact verification. It
    /// must not be retried blindly because a tab may have been created.
    pub fn attempted_unverified(&self) -> Option<&FolderRequestStep> {
        self.attempted_unverified.as_ref()
    }

    /// Paths that have not yet been handed to the native backend.
    pub fn remaining(&self) -> &[FolderPath] {
        &self.remaining
    }

    pub fn outcome(&self) -> OperationOutcome {
        self.outcome
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationPhase {
    ReadyInitial,
    ReadyAppend,
    RequestIssuedInitial,
    RequestIssuedAppend,
    AwaitingInitialEvidence,
    AwaitingAppendEvidence,
    AwaitingFinalVerification,
    Terminal(OperationOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationAction {
    Request(FolderRequestStep),
    WaitingForNativeResult,
    WaitingForObservation,
    VerifyFinal,
    Verified(OperationReport),
    Finished(OperationReport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTabOperation {
    request: FolderTabRequest,
    operation_id: OperationId,
    bound_target: Option<BoundTargetEvidence>,
    verified_prefix: Vec<VerifiedFolderTab>,
    outstanding: Option<FolderRequestStep>,
    phase: OperationPhase,
}

impl FolderTabOperation {
    pub fn new(request: FolderTabRequest, operation_id: OperationId) -> Self {
        Self {
            request,
            operation_id,
            bound_target: None,
            verified_prefix: Vec::new(),
            outstanding: None,
            phase: OperationPhase::ReadyInitial,
        }
    }

    /// Checks control before yielding a native request. The caller must check
    /// control again immediately before and after its backend call.
    pub fn next_action(&mut self, control: OperationControl) -> OperationAction {
        if let OperationPhase::Terminal(_) = self.phase {
            return OperationAction::Finished(self.report());
        }
        if let Some(outcome) = control_outcome(control) {
            self.phase = OperationPhase::Terminal(outcome);
            return OperationAction::Finished(self.report());
        }

        let kind = match self.phase {
            OperationPhase::ReadyInitial => FolderRequestKind::CreateInitialWindow,
            OperationPhase::ReadyAppend => FolderRequestKind::OpenAdditionalTab,
            OperationPhase::RequestIssuedInitial => {
                return OperationAction::WaitingForNativeResult;
            }
            OperationPhase::RequestIssuedAppend => {
                return OperationAction::WaitingForNativeResult;
            }
            OperationPhase::AwaitingInitialEvidence | OperationPhase::AwaitingAppendEvidence => {
                return OperationAction::WaitingForObservation;
            }
            OperationPhase::AwaitingFinalVerification => return OperationAction::VerifyFinal,
            OperationPhase::Terminal(_) => return OperationAction::Finished(self.report()),
        };
        if self.verified_prefix.len() == self.request.len() {
            self.phase = OperationPhase::AwaitingFinalVerification;
            return OperationAction::VerifyFinal;
        }

        let request_index = self.verified_prefix.len();
        let expected_ordinal = match u16::try_from(request_index + 1) {
            Ok(ordinal) => ordinal,
            Err(_) => {
                self.phase = OperationPhase::Terminal(OperationOutcome::Failed(
                    OperationFailure::UnexpectedTabOrdinal,
                ));
                return OperationAction::Finished(self.report());
            }
        };
        let step = FolderRequestStep {
            operation_id: self.operation_id,
            request_index,
            expected_ordinal,
            kind,
            folder: self.request.folders[request_index].clone(),
        };
        self.outstanding = Some(step.clone());
        self.phase = match kind {
            FolderRequestKind::CreateInitialWindow => OperationPhase::RequestIssuedInitial,
            FolderRequestKind::OpenAdditionalTab => OperationPhase::RequestIssuedAppend,
        };
        OperationAction::Request(step)
    }

    /// A successful shell/UI request is not success evidence. It only moves
    /// the operation to the independent observation phase.
    pub fn request_returned(
        &mut self,
        step: &FolderRequestStep,
        result: RequestResult,
        control_after: OperationControl,
    ) -> OperationAction {
        if let OperationPhase::Terminal(_) = self.phase {
            return OperationAction::Finished(self.report());
        }
        if let Some(outcome) = control_outcome(control_after) {
            self.phase = OperationPhase::Terminal(outcome);
            return OperationAction::Finished(self.report());
        }
        if self.outstanding.as_ref() != Some(step) {
            self.phase = OperationPhase::Terminal(OperationOutcome::Failed(
                OperationFailure::StaleRequestResult,
            ));
            return OperationAction::Finished(self.report());
        }
        let expected_phase = match step.kind {
            FolderRequestKind::CreateInitialWindow => OperationPhase::RequestIssuedInitial,
            FolderRequestKind::OpenAdditionalTab => OperationPhase::RequestIssuedAppend,
        };
        if self.phase != expected_phase || step.operation_id != self.operation_id {
            self.phase = OperationPhase::Terminal(OperationOutcome::Failed(
                OperationFailure::StaleRequestResult,
            ));
            return OperationAction::Finished(self.report());
        }
        if result == RequestResult::Rejected {
            self.phase = OperationPhase::Terminal(OperationOutcome::Failed(
                OperationFailure::RequestRejected,
            ));
            return OperationAction::Finished(self.report());
        }
        self.phase = match step.kind {
            FolderRequestKind::CreateInitialWindow => OperationPhase::AwaitingInitialEvidence,
            FolderRequestKind::OpenAdditionalTab => OperationPhase::AwaitingAppendEvidence,
        };
        OperationAction::WaitingForObservation
    }

    pub fn observe_initial_target(
        &mut self,
        evidence: Result<InitialTargetEvidence, SnapshotFailure>,
        control_after: OperationControl,
    ) -> OperationAction {
        if let OperationPhase::Terminal(_) = self.phase {
            return OperationAction::Finished(self.report());
        }
        if let Some(outcome) = control_outcome(control_after) {
            self.phase = OperationPhase::Terminal(outcome);
            return OperationAction::Finished(self.report());
        }
        let Some(step) = self.outstanding.clone() else {
            return self.fail(OperationFailure::UnexpectedObservation);
        };
        if self.phase != OperationPhase::AwaitingInitialEvidence
            || step.kind != FolderRequestKind::CreateInitialWindow
        {
            return self.fail(OperationFailure::UnexpectedObservation);
        }
        let evidence = match evidence {
            Ok(evidence) => evidence,
            Err(failure) => {
                return self.fail(OperationFailure::SnapshotIncomplete(failure));
            }
        };
        if evidence.bound_target.operation_id != self.operation_id {
            return self.fail(OperationFailure::WrongTargetEvidence);
        }
        if evidence.ordinal != 1 {
            return self.fail(OperationFailure::UnexpectedTabOrdinal);
        }
        if !folder_evidence_matches(&evidence.folder, &step) {
            return self.fail(folder_evidence_failure(&evidence.folder, &step));
        }

        self.bound_target = Some(evidence.bound_target.clone());
        self.verified_prefix.push(VerifiedFolderTab {
            request_index: step.request_index,
            folder: step.folder,
            shell_binding: evidence.shell_binding,
            identity: evidence.tab_identity,
            ordinal: evidence.ordinal,
        });
        self.outstanding = None;
        self.after_verified_step()
    }

    pub fn observe_additional_tab(
        &mut self,
        evidence: Result<VerifiedTabEvidence, SnapshotFailure>,
        control_after: OperationControl,
    ) -> OperationAction {
        if let OperationPhase::Terminal(_) = self.phase {
            return OperationAction::Finished(self.report());
        }
        if let Some(outcome) = control_outcome(control_after) {
            self.phase = OperationPhase::Terminal(outcome);
            return OperationAction::Finished(self.report());
        }
        let Some(step) = self.outstanding.clone() else {
            return self.fail(OperationFailure::UnexpectedObservation);
        };
        if self.phase != OperationPhase::AwaitingAppendEvidence
            || step.kind != FolderRequestKind::OpenAdditionalTab
        {
            return self.fail(OperationFailure::UnexpectedObservation);
        }
        let evidence = match evidence {
            Ok(evidence) => evidence,
            Err(failure) => {
                return self.fail(OperationFailure::SnapshotIncomplete(failure));
            }
        };
        if evidence.bound_target.operation_id != self.operation_id
            || self.bound_target.as_ref() != Some(&evidence.bound_target)
        {
            return self.fail(OperationFailure::TargetChanged);
        }
        if evidence.ordinal != step.expected_ordinal {
            return self.fail(OperationFailure::UnexpectedTabOrdinal);
        }
        if self
            .verified_prefix
            .iter()
            .any(|verified| verified.identity == evidence.identity)
        {
            return self.fail(OperationFailure::ReusedTabIdentity);
        }
        if !folder_evidence_matches(&evidence.folder, &step) {
            return self.fail(folder_evidence_failure(&evidence.folder, &step));
        }

        self.verified_prefix.push(VerifiedFolderTab {
            request_index: step.request_index,
            folder: step.folder,
            shell_binding: evidence.shell_binding,
            identity: evidence.identity,
            ordinal: evidence.ordinal,
        });
        self.outstanding = None;
        self.after_verified_step()
    }

    pub fn report(&self) -> OperationReport {
        let outcome = match self.phase {
            OperationPhase::Terminal(outcome) => outcome,
            _ => OperationOutcome::Running,
        };
        let remaining_from = self
            .outstanding
            .as_ref()
            .map(|step| step.request_index.saturating_add(1))
            .unwrap_or(self.verified_prefix.len());
        OperationReport {
            verified_prefix: self.verified_prefix.clone(),
            attempted_unverified: self.outstanding.clone(),
            remaining: self.request.folders[remaining_from..].to_vec(),
            outcome,
        }
    }

    fn fail(&mut self, failure: OperationFailure) -> OperationAction {
        self.phase = OperationPhase::Terminal(OperationOutcome::Failed(failure));
        OperationAction::Finished(self.report())
    }

    fn after_verified_step(&mut self) -> OperationAction {
        if self.verified_prefix.len() == self.request.len() {
            self.phase = OperationPhase::AwaitingFinalVerification;
            OperationAction::VerifyFinal
        } else {
            self.phase = OperationPhase::ReadyAppend;
            OperationAction::Verified(self.report())
        }
    }

    pub(crate) fn final_verification_returned(
        &mut self,
        valid: bool,
        control: OperationControl,
    ) -> OperationAction {
        if let OperationPhase::Terminal(_) = self.phase {
            return OperationAction::Finished(self.report());
        }
        if let Some(outcome) = control_outcome(control) {
            self.phase = OperationPhase::Terminal(outcome);
        } else if self.phase != OperationPhase::AwaitingFinalVerification {
            return self.fail(OperationFailure::UnexpectedObservation);
        } else if valid {
            self.phase = OperationPhase::Terminal(OperationOutcome::Completed);
        } else {
            self.phase = OperationPhase::Terminal(OperationOutcome::Failed(
                OperationFailure::FinalVerificationFailed,
            ));
        }
        OperationAction::Finished(self.report())
    }

    pub(crate) fn backend_failure(&mut self, failure: OperationFailure) -> OperationAction {
        if matches!(self.phase, OperationPhase::Terminal(_)) {
            return OperationAction::Finished(self.report());
        }
        self.fail(failure)
    }

    pub(crate) fn backend_failure_before_dispatch(
        &mut self,
        failure: OperationFailure,
    ) -> OperationAction {
        if matches!(
            self.phase,
            OperationPhase::RequestIssuedInitial | OperationPhase::RequestIssuedAppend
        ) {
            self.outstanding = None;
        }
        self.backend_failure(failure)
    }

    /// A step is only an attempted native request once its call may have run.
    pub(crate) fn stop_before_dispatch(&mut self, control: OperationControl) -> OperationAction {
        if matches!(
            self.phase,
            OperationPhase::RequestIssuedInitial | OperationPhase::RequestIssuedAppend
        ) && control != OperationControl::Continue
        {
            self.outstanding = None;
        }
        self.next_action(control)
    }
}

fn folder_evidence_matches(evidence: &FolderIdentityEvidence, step: &FolderRequestStep) -> bool {
    evidence.request_index == step.request_index
        && evidence.requested_path == step.folder
        && evidence.result == FolderIdentityResult::ExactPidl
}

fn folder_evidence_failure(
    evidence: &FolderIdentityEvidence,
    step: &FolderRequestStep,
) -> OperationFailure {
    if evidence.request_index != step.request_index || evidence.requested_path != step.folder {
        OperationFailure::WrongRequestEvidence
    } else {
        match evidence.result {
            FolderIdentityResult::ExactPidl => OperationFailure::WrongRequestEvidence,
            FolderIdentityResult::DifferentPidl => OperationFailure::WrongFolder,
            FolderIdentityResult::Unavailable => OperationFailure::FolderIdentityUnavailable,
        }
    }
}

fn control_outcome(control: OperationControl) -> Option<OperationOutcome> {
    match control {
        OperationControl::Continue => None,
        OperationControl::Cancelled => Some(OperationOutcome::Cancelled),
        OperationControl::DeadlineExpired => Some(OperationOutcome::DeadlineExpired),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::explorer_tabs::identity::test_support as identity_support;

    pub(crate) fn operation_id(value: u64) -> OperationId {
        OperationId::from_token(value).expect("nonzero fixture operation id")
    }

    pub(crate) fn verified_identity(value: u64) -> VerifiedTabIdentity {
        VerifiedTabIdentity::from_worker_token(value).expect("nonzero fixture tab id")
    }

    fn target_authority(operation_id: OperationId) -> TargetAuthorityId {
        TargetAuthorityId::from_correlated_worker_token(
            operation_id
                .0
                .get()
                .checked_add(1_000_000)
                .expect("fixture authority token remains bounded"),
        )
        .expect("nonzero fixture authority token")
    }

    fn bound_target(operation_id: OperationId, window: WindowIdentity) -> BoundTargetEvidence {
        BoundTargetEvidence::from_correlated_launch(
            operation_id,
            target_authority(operation_id),
            window,
        )
    }

    pub(crate) fn initial_evidence(
        operation_id: OperationId,
        step: &FolderRequestStep,
        hwnd: usize,
        binding: u64,
        tab_identity: u64,
        result: FolderIdentityResult,
    ) -> InitialTargetEvidence {
        let window = identity_support::window(hwnd);
        let bound_target = bound_target(operation_id, window);
        InitialTargetEvidence::verified(
            bound_target,
            identity_support::shell_binding(binding),
            verified_identity(tab_identity),
            1,
            FolderIdentityEvidence::from_pidl_check(
                step.request_index,
                step.folder.clone(),
                result,
            ),
        )
    }

    pub(crate) fn tab_evidence(
        operation_id: OperationId,
        target: WindowIdentity,
        binding: u64,
        step: &FolderRequestStep,
        tab_identity: u64,
        ordinal: u16,
        result: FolderIdentityResult,
    ) -> VerifiedTabEvidence {
        VerifiedTabEvidence::verified(
            bound_target(operation_id, target),
            identity_support::shell_binding(binding),
            verified_identity(tab_identity),
            ordinal,
            FolderIdentityEvidence::from_pidl_check(
                step.request_index,
                step.folder.clone(),
                result,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::test_support as operation_support;
    use super::*;
    use crate::explorer_tabs::identity::test_support as identity_support;

    fn path(value: &str) -> FolderPath {
        FolderPath::parse(value).expect("valid test path")
    }

    fn request(paths: &[&str]) -> FolderTabRequest {
        FolderTabRequest::new(paths.iter().map(|value| path(value)).collect()).unwrap()
    }

    fn next_request(operation: &mut FolderTabOperation) -> FolderRequestStep {
        match operation.next_action(OperationControl::Continue) {
            OperationAction::Request(step) => step,
            other => panic!("expected request, received {other:?}"),
        }
    }

    fn start_operation(
        paths: &[&str],
        id: u64,
    ) -> (FolderTabOperation, OperationId, FolderRequestStep) {
        let operation_id = operation_support::operation_id(id);
        let mut operation = FolderTabOperation::new(request(paths), operation_id);
        let step = next_request(&mut operation);
        assert_eq!(step.kind(), FolderRequestKind::CreateInitialWindow);
        assert_eq!(step.expected_ordinal(), 1);
        (operation, operation_id, step)
    }

    fn verify_initial(
        operation: &mut FolderTabOperation,
        operation_id: OperationId,
        step: &FolderRequestStep,
    ) {
        assert_eq!(
            operation.request_returned(step, RequestResult::Accepted, OperationControl::Continue),
            OperationAction::WaitingForObservation
        );
        operation.observe_initial_target(
            Ok(operation_support::initial_evidence(
                operation_id,
                step,
                10,
                20,
                30,
                FolderIdentityResult::ExactPidl,
            )),
            OperationControl::Continue,
        );
    }

    #[test]
    fn explorer_tabs_operation_preserves_unicode_spaces_and_duplicate_order() {
        let request = request(&[r"C:\Résumé projects\東京", r"C:\Résumé projects\東京"]);
        assert_eq!(request.len(), 2);
        assert_eq!(request.folders()[0], request.folders()[1]);
        assert_eq!(request.folders()[0].as_str(), r"C:\Résumé projects\東京");
    }

    #[test]
    fn explorer_tabs_operation_rejects_relative_device_and_invalid_paths() {
        assert_eq!(
            FolderPath::parse(r"C:relative"),
            Err(FolderPathError::RelativeDrivePath)
        );
        assert_eq!(
            FolderPath::parse(r"folder\relative"),
            Err(FolderPathError::NotAbsolute)
        );
        assert_eq!(
            FolderPath::parse(r"\\?\C:\device"),
            Err(FolderPathError::DeviceNamespace)
        );
        assert_eq!(
            FolderPath::parse("C:\\bad\0path"),
            Err(FolderPathError::ControlCharacter)
        );
        assert_eq!(
            FolderPath::parse(r"\server\share"),
            Err(FolderPathError::NotAbsolute)
        );
        assert_eq!(
            FolderPath::parse(r"\\server\"),
            Err(FolderPathError::InvalidUncPath)
        );
    }

    #[test]
    fn explorer_tabs_operation_caps_request_count_and_total_bytes() {
        let too_many = vec![path(r"C:\x"); MAX_REQUEST_FOLDERS + 1];
        assert_eq!(
            FolderTabRequest::new(too_many),
            Err(FolderRequestError::TooManyFolders)
        );
        let per_path = MAX_FOLDER_PATH_UTF16_UNITS - 4;
        let huge_path = format!("C:\\{}", "a".repeat(per_path));
        let folders: Vec<_> = (0..9)
            .map(|index| FolderPath::parse(&format!("{huge_path}{index}")).unwrap())
            .collect();
        assert!(folders.iter().map(FolderPath::utf8_len).sum::<usize>() > MAX_REQUEST_PATH_BYTES);
        assert_eq!(
            FolderTabRequest::new(folders),
            Err(FolderRequestError::PathBytesExceeded)
        );
    }

    #[test]
    fn explorer_tabs_operation_initial_path_is_first_tab_and_later_path_appends() {
        let (mut operation, operation_id, initial) = start_operation(&[r"C:\one", r"C:\two"], 1);
        verify_initial(&mut operation, operation_id, &initial);
        let append = match operation.next_action(OperationControl::Continue) {
            OperationAction::Request(step) => step,
            other => panic!("expected append request, received {other:?}"),
        };
        assert_eq!(append.kind(), FolderRequestKind::OpenAdditionalTab);
        assert_eq!(append.expected_ordinal(), 2);
        assert_eq!(append.folder(), &path(r"C:\two"));
    }

    #[test]
    fn explorer_tabs_operation_rejects_wrong_target_path_and_ordinal_evidence() {
        let (mut wrong_target, operation_id, initial) = start_operation(&[r"C:\one", r"C:\two"], 2);
        wrong_target.request_returned(
            &initial,
            RequestResult::Accepted,
            OperationControl::Continue,
        );
        wrong_target.observe_initial_target(
            Ok(operation_support::initial_evidence(
                operation_id,
                &initial,
                10,
                20,
                30,
                FolderIdentityResult::ExactPidl,
            )),
            OperationControl::Continue,
        );
        let append = next_request(&mut wrong_target);
        wrong_target.request_returned(&append, RequestResult::Accepted, OperationControl::Continue);
        let reused_hwnd = identity_support::changed_process(identity_support::window(10), 101, 201);
        wrong_target.observe_additional_tab(
            Ok(operation_support::tab_evidence(
                operation_id,
                reused_hwnd,
                20,
                &append,
                31,
                2,
                FolderIdentityResult::ExactPidl,
            )),
            OperationControl::Continue,
        );
        assert_eq!(
            wrong_target.report().outcome(),
            OperationOutcome::Failed(OperationFailure::TargetChanged)
        );
        assert_eq!(wrong_target.report().verified_prefix().len(), 1);

        let (mut wrong_folder, operation_id, initial) = start_operation(&[r"C:\one"], 3);
        wrong_folder.request_returned(
            &initial,
            RequestResult::Accepted,
            OperationControl::Continue,
        );
        wrong_folder.observe_initial_target(
            Ok(operation_support::initial_evidence(
                operation_id,
                &initial,
                10,
                20,
                30,
                FolderIdentityResult::DifferentPidl,
            )),
            OperationControl::Continue,
        );
        assert_eq!(
            wrong_folder.report().outcome(),
            OperationOutcome::Failed(OperationFailure::WrongFolder)
        );

        let (mut wrong_ordinal, operation_id, initial) = start_operation(&[r"C:\one"], 4);
        wrong_ordinal.request_returned(
            &initial,
            RequestResult::Accepted,
            OperationControl::Continue,
        );
        let mut evidence = operation_support::initial_evidence(
            operation_id,
            &initial,
            10,
            20,
            30,
            FolderIdentityResult::ExactPidl,
        );
        evidence.ordinal = 2;
        wrong_ordinal.observe_initial_target(Ok(evidence), OperationControl::Continue);
        assert_eq!(
            wrong_ordinal.report().outcome(),
            OperationOutcome::Failed(OperationFailure::UnexpectedTabOrdinal)
        );
    }

    #[test]
    fn explorer_tabs_operation_rejects_reused_tab_identity_and_preserves_prefix() {
        let (mut operation, operation_id, initial) =
            start_operation(&[r"C:\one", r"C:\two", r"C:\three", r"C:\four"], 5);
        verify_initial(&mut operation, operation_id, &initial);
        let second = next_request(&mut operation);
        operation.request_returned(&second, RequestResult::Accepted, OperationControl::Continue);
        let target = identity_support::window(10);
        let second_evidence = operation_support::tab_evidence(
            operation_id,
            target,
            21,
            &second,
            31,
            2,
            FolderIdentityResult::ExactPidl,
        );
        assert!(matches!(
            operation.observe_additional_tab(Ok(second_evidence), OperationControl::Continue),
            OperationAction::Verified(_)
        ));
        assert_eq!(operation.report().verified_prefix().len(), 2);
        assert_ne!(
            operation.report().verified_prefix()[0].shell_binding(),
            operation.report().verified_prefix()[1].shell_binding()
        );

        let third = next_request(&mut operation);
        operation.request_returned(&third, RequestResult::Accepted, OperationControl::Continue);
        operation.observe_additional_tab(
            Ok(operation_support::tab_evidence(
                operation_id,
                target,
                20,
                &third,
                31,
                3,
                FolderIdentityResult::ExactPidl,
            )),
            OperationControl::Continue,
        );
        assert_eq!(
            operation.report().outcome(),
            OperationOutcome::Failed(OperationFailure::ReusedTabIdentity)
        );
        assert_eq!(operation.report().verified_prefix().len(), 2);
        assert_eq!(operation.report().attempted_unverified(), Some(&third));
        assert_eq!(operation.report().remaining(), &[path(r"C:\four")]);
    }

    #[test]
    fn explorer_tabs_operation_binds_path_evidence_to_duplicate_step() {
        let (mut operation, operation_id, initial) = start_operation(&[r"C:\same", r"C:\same"], 6);
        verify_initial(&mut operation, operation_id, &initial);
        let second = next_request(&mut operation);
        operation.request_returned(&second, RequestResult::Accepted, OperationControl::Continue);
        let mut evidence = operation_support::tab_evidence(
            operation_id,
            identity_support::window(10),
            20,
            &second,
            32,
            2,
            FolderIdentityResult::ExactPidl,
        );
        evidence.folder.request_index = 0;
        operation.observe_additional_tab(Ok(evidence), OperationControl::Continue);
        assert_eq!(
            operation.report().outcome(),
            OperationOutcome::Failed(OperationFailure::WrongRequestEvidence)
        );
        assert_eq!(operation.report().verified_prefix().len(), 1);
    }

    #[test]
    fn explorer_tabs_operation_rejects_wrong_target_authority() {
        let (mut operation, operation_id, initial) = start_operation(&[r"C:\one", r"C:\two"], 18);
        verify_initial(&mut operation, operation_id, &initial);
        let append = next_request(&mut operation);
        operation.request_returned(&append, RequestResult::Accepted, OperationControl::Continue);
        let mut evidence = operation_support::tab_evidence(
            operation_id,
            identity_support::window(10),
            22,
            &append,
            31,
            2,
            FolderIdentityResult::ExactPidl,
        );
        evidence.bound_target.authority_id =
            TargetAuthorityId::from_correlated_worker_token(999_999)
                .expect("nonzero fixture target authority");
        operation.observe_additional_tab(Ok(evidence), OperationControl::Continue);

        assert_eq!(
            operation.report().outcome(),
            OperationOutcome::Failed(OperationFailure::TargetChanged)
        );
        assert_eq!(operation.report().verified_prefix().len(), 1);
    }

    #[test]
    fn explorer_tabs_operation_preserves_partial_prefix_and_separates_uncertain_work() {
        let (mut operation, operation_id, initial) =
            start_operation(&[r"C:\one", r"C:\two", r"C:\three"], 7);
        verify_initial(&mut operation, operation_id, &initial);
        let second = next_request(&mut operation);
        operation.request_returned(&second, RequestResult::Accepted, OperationControl::Continue);
        operation.observe_additional_tab(
            Err(SnapshotFailure::ActiveViewUnavailable { index: 0 }),
            OperationControl::Continue,
        );
        let report = operation.report();
        assert_eq!(report.verified_prefix().len(), 1);
        assert_eq!(report.attempted_unverified(), Some(&second));
        assert_eq!(report.remaining(), &[path(r"C:\three")]);
    }

    #[test]
    fn explorer_tabs_operation_does_not_treat_request_return_as_verified_success() {
        let (mut operation, _, step) = start_operation(&[r"C:\one", r"C:\two"], 12);
        operation.request_returned(&step, RequestResult::Accepted, OperationControl::Continue);
        let report = operation.report();
        assert_eq!(report.outcome(), OperationOutcome::Running);
        assert!(report.verified_prefix().is_empty());
        assert_eq!(report.attempted_unverified(), Some(&step));
        assert_eq!(report.remaining(), &[path(r"C:\two")]);

        let (mut rejected, _, step) = start_operation(&[r"C:\one", r"C:\two"], 13);
        rejected.request_returned(&step, RequestResult::Rejected, OperationControl::Continue);
        assert_eq!(
            rejected.report().outcome(),
            OperationOutcome::Failed(OperationFailure::RequestRejected)
        );
        assert_eq!(rejected.report().attempted_unverified(), Some(&step));
        assert_eq!(rejected.report().remaining(), &[path(r"C:\two")]);
    }

    #[test]
    fn explorer_tabs_operation_rejects_stale_request_and_foreign_operation_evidence() {
        let (mut stale_request, _, step) = start_operation(&[r"C:\one"], 14);
        let mut foreign_step = step.clone();
        foreign_step.operation_id = operation_support::operation_id(15);
        stale_request.request_returned(
            &foreign_step,
            RequestResult::Accepted,
            OperationControl::Continue,
        );
        assert_eq!(
            stale_request.report().outcome(),
            OperationOutcome::Failed(OperationFailure::StaleRequestResult)
        );

        let (mut foreign_evidence, operation_id, initial) = start_operation(&[r"C:\one"], 16);
        foreign_evidence.request_returned(
            &initial,
            RequestResult::Accepted,
            OperationControl::Continue,
        );
        foreign_evidence.observe_initial_target(
            Ok(operation_support::initial_evidence(
                operation_support::operation_id(17),
                &initial,
                10,
                20,
                30,
                FolderIdentityResult::ExactPidl,
            )),
            OperationControl::Continue,
        );
        assert_eq!(
            foreign_evidence.report().outcome(),
            OperationOutcome::Failed(OperationFailure::WrongTargetEvidence)
        );
        assert_eq!(foreign_evidence.report().verified_prefix().len(), 0);
        assert_eq!(
            foreign_evidence.report().attempted_unverified(),
            Some(&initial)
        );
        assert_ne!(operation_id, operation_support::operation_id(17));
    }

    #[test]
    fn explorer_tabs_operation_freezes_terminal_outcomes_against_late_results() {
        let (mut operation, operation_id, initial) = start_operation(&[r"C:\one"], 8);
        verify_initial(&mut operation, operation_id, &initial);
        assert_eq!(operation.report().outcome(), OperationOutcome::Running);
        assert_eq!(
            operation.next_action(OperationControl::Continue),
            OperationAction::VerifyFinal
        );
        operation.final_verification_returned(true, OperationControl::Continue);
        let completed = operation.report();
        assert_eq!(completed.outcome(), OperationOutcome::Completed);
        assert_eq!(
            operation.next_action(OperationControl::Cancelled),
            OperationAction::Finished(completed.clone())
        );
        assert_eq!(operation.report(), completed);
        assert_eq!(
            operation.request_returned(
                &initial,
                RequestResult::Rejected,
                OperationControl::Continue
            ),
            OperationAction::Finished(completed.clone())
        );
        assert_eq!(operation.report(), completed);
        assert_eq!(
            operation.observe_initial_target(
                Err(SnapshotFailure::ShellUnavailable),
                OperationControl::Continue
            ),
            OperationAction::Finished(completed)
        );

        let (mut cancelled, _, _) = start_operation(&[r"C:\one", r"C:\two"], 9);
        cancelled.next_action(OperationControl::Cancelled);
        let cancelled_report = cancelled.report();
        assert_eq!(
            cancelled.next_action(OperationControl::DeadlineExpired),
            OperationAction::Finished(cancelled_report.clone())
        );
        assert_eq!(cancelled.report(), cancelled_report);
    }

    #[test]
    fn explorer_tabs_operation_checks_cancellation_and_deadline_around_calls() {
        let mut cancelled = FolderTabOperation::new(
            request(&[r"C:\one", r"C:\two"]),
            operation_support::operation_id(10),
        );
        assert!(matches!(
            cancelled.next_action(OperationControl::Cancelled),
            OperationAction::Finished(_)
        ));
        assert_eq!(cancelled.report().outcome(), OperationOutcome::Cancelled);
        assert_eq!(cancelled.report().attempted_unverified(), None);
        assert_eq!(cancelled.report().remaining().len(), 2);

        let (mut deadline, _, step) = start_operation(&[r"C:\one", r"C:\two"], 11);
        deadline.request_returned(
            &step,
            RequestResult::Accepted,
            OperationControl::DeadlineExpired,
        );
        assert_eq!(
            deadline.report().outcome(),
            OperationOutcome::DeadlineExpired
        );
        assert_eq!(deadline.report().attempted_unverified(), Some(&step));
        assert_eq!(deadline.report().remaining().len(), 1);
    }
}
