//! Typed in-memory notification Inbox.
mod model;
mod store;
mod windows_consent;
pub(crate) use model::*;
pub(crate) use store::{HISTORY_FILE, InboxStore, probe_history};
pub(crate) use windows_consent::{WindowsAccess, WindowsConsent};
mod actions;
mod producers;
pub(crate) use actions::create_todo_from_action;
pub(crate) use producers::{attach_activity_inbox, publish_hot_folder, unix_now};

mod service;
pub(crate) use service::{
    InboxCommand, InboxCommandOutcome, InboxCommandReceipt, InboxPublisher, InboxService,
    InboxServiceSnapshot, InternalEvent, PersistenceStatus, SubmitError,
};
