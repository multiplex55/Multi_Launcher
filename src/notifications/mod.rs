//! Typed in-memory notification Inbox.
mod model;
mod store;
pub(crate) use model::*;
pub(crate) use store::{HISTORY_FILE, InboxStore, probe_history};
mod actions;
pub(crate) use actions::create_todo_from_action;
