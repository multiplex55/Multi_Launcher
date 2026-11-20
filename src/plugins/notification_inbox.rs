use crate::actions::Action;
use crate::plugin::Plugin;

pub struct NotificationInboxPlugin;

impl Plugin for NotificationInboxPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        if !["inbox", "notifications", "notification inbox"]
            .iter()
            .any(|key| query.eq_ignore_ascii_case(key))
        {
            return Vec::new();
        }
        vec![Action {
            label: "Open Notification Inbox".into(),
            desc: "Review launcher alerts, reminders, and completed work".into(),
            action: "notification_inbox:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "notification_inbox"
    }

    fn description(&self) -> &str {
        "Review notifications (prefix: inbox)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["inbox", "notifications", "notification"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "inbox".into(),
            desc: "Open Notification Inbox".into(),
            action: "query:inbox".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, DialogCommand, parse_action};

    #[test]
    fn notification_inbox_plugin_opens_typed_dialog_and_rejects_payload() {
        for query in ["Inbox", " notifications ", "notification inbox"] {
            let actions = NotificationInboxPlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Dialog(DialogCommand::NotificationInbox)
            );
        }
        assert!(
            NotificationInboxPlugin
                .search("notification inbox private")
                .is_empty()
        );
        let mut action = NotificationInboxPlugin.search("inbox").pop().unwrap();
        action.args = Some("PRIVATE_SENTINEL".into());
        assert!(
            !parse_action(&action)
                .unwrap_err()
                .to_string()
                .contains("PRIVATE_SENTINEL")
        );
    }
}
