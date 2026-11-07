use crate::actions::Action;
use crate::plugin::Plugin;

pub struct FileLockPlugin;

impl Plugin for FileLockPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("filelock") {
            return Vec::new();
        }

        vec![Action {
            label: "Inspect File Users".into(),
            desc: "See which processes Windows reports as using a selected file".into(),
            action: "filelock:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "file_lock"
    }

    fn description(&self) -> &str {
        "Read-only inspection of processes reported for a selected file (query: `filelock`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["filelock"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "filelock".into(),
            desc: "Inspect reported file users".into(),
            action: "query:filelock".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_lock_ui_query_is_exact_and_routes_to_the_typed_dialog() {
        let plugin = FileLockPlugin;
        assert!(plugin.search("filelock extra").is_empty());
        assert!(plugin.search("file lock").is_empty());

        let matches = plugin.search("  FileLock  ");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].action, "filelock:open");
        assert_eq!(matches[0].args, None);
        assert_eq!(
            crate::commands::parse_action(&matches[0]).unwrap(),
            crate::commands::Command::Dialog(crate::commands::DialogCommand::FileLock {
                path: None,
            })
        );
        assert_eq!(plugin.commands()[0].action, "query:filelock");
    }
}
