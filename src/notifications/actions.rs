//! Routes explicit Inbox action tickets to their domain owner.
use super::{ActionKind, ActionTicket, Content, Inbox};
use crate::platform::app_data::AppDataRoot;
use crate::plugins::todo::{TODO_FILE, insert_notification_task};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

pub(crate) fn create_todo_from_action(
    profile: &AppDataRoot,
    inbox: &Inbox,
    ticket: &ActionTicket,
) -> Result<String> {
    ensure!(
        ticket.kind == ActionKind::CreateTodo && inbox.action_is_current(ticket),
        "This Inbox action is no longer current."
    );
    let record = inbox
        .records()
        .iter()
        .find(|row| row.id == ticket.id)
        .context("This notification is no longer available.")?;
    let text = match record.content {
        Content::Internal(fact) => format!("Follow up on {}", fact.summary()),
        Content::External { .. } => "Follow up on Windows notification".into(),
    };
    let key = serde_json::to_vec(&ticket.id).context("Identify notification task")?;
    let id = format!("todo-notification-{:x}", Sha256::digest(key));
    let location = profile.path().join(TODO_FILE);
    let path = location
        .to_str()
        .context("The Todo profile path is not supported.")?;
    insert_notification_task(path, &id, text)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::{Category, FactId, InternalFact, Status};

    #[test]
    fn notification_inbox_create_todo_is_idempotent_across_action_retries() {
        let directory = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(directory.path());
        let mut inbox = Inbox::default();
        let source = FactId::Launcher {
            session: [7; 16],
            sequence: 1,
        };
        inbox.ingest_internal(
            source.clone(),
            12,
            Category::Information,
            Status::Current,
            InternalFact::LauncherInfo,
            None,
            12,
        );
        let ticket = inbox.begin_action(&source, ActionKind::CreateTodo).unwrap();
        let id = create_todo_from_action(&profile, &inbox, &ticket).unwrap();
        assert!(inbox.finish_action(&ticket, false));
        assert!(create_todo_from_action(&profile, &inbox, &ticket).is_err());

        let retry = inbox.begin_action(&source, ActionKind::CreateTodo).unwrap();
        assert_eq!(
            create_todo_from_action(&profile, &inbox, &retry).unwrap(),
            id
        );
        assert!(inbox.finish_action(&retry, true));
        let path = profile.path().join(TODO_FILE);
        let stored = crate::plugins::todo::load_todos(path.to_str().unwrap()).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, id);
    }
}
