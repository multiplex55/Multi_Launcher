//! Disabled-by-default Hot Folder rules and a bounded watcher/settle runtime.
//!
//! The watcher observes reported paths and transfers settled evidence to the
//! guarded execution owner. Built-in file actions use shared native guards;
//! external action dispatch is supplied through an explicit adapter.

mod capability;
mod execution;
mod model;
mod runtime;
mod store;

pub use capability::{
    ActionCapability, ActionEnablement, HotFolderCapabilityContext, RootAvailability,
    bind_mkmacro_action, bind_registered_launcher_action, evaluate_action_capability,
};
pub use execution::{
    HotFolderExecutionAction, HotFolderExecutionEngine, HotFolderExecutionError,
    HotFolderExecutionPhase, HotFolderExecutionReceipt, HotFolderExecutionSnapshot,
    HotFolderExternalActionAdapter, HotFolderExternalActionOutcome, HotFolderExternalActionRequest,
    HotFolderOutputEvidence, HotFolderOutputKind, HotFolderRunDisposition, HotFolderTerminalStatus,
};
pub use model::{
    ConfigurationFingerprint, FilenameFilter, HOT_FOLDER_RULES_FILE,
    HOT_FOLDER_RULES_FORMAT_VERSION, HotFolderRule, MAX_DOCUMENT_BYTES,
    MAX_DUE_OBSERVATIONS_PER_TURN, MAX_EVENT_PATH_BYTES, MAX_EVENT_PATHS, MAX_EXTENSIONS,
    MAX_FILENAME_GLOB_CHARS, MAX_PENDING_FILES, MAX_QUARANTINED_ROWS, MAX_READY_CANDIDATES,
    MAX_RULE_PATH_CHARS, MAX_RULES, MAX_RUNTIME_DIAGNOSTICS, MAX_WATCH_INGRESS_EVENTS,
    MAX_WATCHED_ROOTS, READY_LEASE_TIMEOUT_MS, RuleAction, RuleId, RuleTrigger, SettlePolicy,
};
pub use runtime::{
    HotFolderCandidateId, HotFolderDiagnostic, HotFolderDiagnosticKind, HotFolderEnableRequest,
    HotFolderFileEvidence, HotFolderLeaseDisposition, HotFolderReadyCandidate, HotFolderReadyLease,
    HotFolderReconcileReport, HotFolderRuntimeError, HotFolderRuntimeSnapshot,
    HotFolderWatchService, StrongActionConfirmation,
};
pub(crate) use store::{HotFolderDocumentProbe, probe_document_bytes};
pub use store::{
    HotFolderLoadReport, HotFolderLoadStatus, HotFolderRulesSnapshot, HotFolderRulesStore,
    HotFolderStoreError, QuarantinedRule,
};
