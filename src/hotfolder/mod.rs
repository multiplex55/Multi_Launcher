//! Disabled-by-default Hot Folder rule definitions and read-only capabilities.
//!
//! This milestone owns definitions and their durable metadata only. It does
//! not install watchers, dispatch launcher actions, or mutate watched files.

mod capability;
mod model;
mod store;

pub use capability::{
    ActionCapability, ActionEnablement, HotFolderCapabilityContext, RootAvailability,
    bind_mkmacro_action, bind_registered_launcher_action, evaluate_action_capability,
};
pub use model::{
    ConfigurationFingerprint, FilenameFilter, HOT_FOLDER_RULES_FILE,
    HOT_FOLDER_RULES_FORMAT_VERSION, HotFolderRule, MAX_DOCUMENT_BYTES, MAX_EXTENSIONS,
    MAX_FILENAME_GLOB_CHARS, MAX_QUARANTINED_ROWS, MAX_RULE_PATH_CHARS, MAX_RULES, RuleAction,
    RuleId, RuleTrigger, SettlePolicy,
};
pub use store::{
    HotFolderLoadReport, HotFolderLoadStatus, HotFolderRulesSnapshot, HotFolderRulesStore,
    HotFolderStoreError, QuarantinedRule,
};
pub(crate) use store::{HotFolderDocumentProbe, probe_document_bytes};
