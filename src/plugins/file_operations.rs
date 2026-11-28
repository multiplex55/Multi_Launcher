use crate::actions::Action;
use crate::plugin::Plugin;

pub struct FileOperationsPlugin;

impl Plugin for FileOperationsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !["fileops", "file operations", "file operations center"]
            .iter()
            .any(|name| query.trim().eq_ignore_ascii_case(name))
        {
            return Vec::new();
        }
        vec![Action {
            label: "Open File Operations Center".into(),
            desc: "Review and manage file transfers".into(),
            action: "file_operations:open".into(),
            args: None,
        }]
    }
    fn name(&self) -> &str {
        <Self as crate::plugin::BuiltinIdentity>::ID
    }
    fn description(&self) -> &str {
        "Review and manage file transfers (prefix: `fileops`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["fileops", "file"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "fileops".into(),
            desc: "Open File Operations Center".into(),
            action: "query:fileops".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, DialogCommand, parse_action};
    #[test]
    fn file_operations_plugin_queries_route_to_typed_dialog() {
        for query in ["fileops", " FILE OPERATIONS ", "File Operations Center"] {
            let actions = FileOperationsPlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].args, None);
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Dialog(DialogCommand::FileOperationsCenter)
            );
        }
        for query in [
            "file",
            "fileops now",
            "file operations center now",
            "file_operations:open",
        ] {
            assert!(FileOperationsPlugin.search(query).is_empty());
        }
        assert_eq!(FileOperationsPlugin.query_prefixes(), ["fileops", "file"]);
    }
    #[test]
    fn file_operations_parser_rejects_payload_and_unknown_without_echo() {
        let sentinel = "PRIVATE_SENTINEL";
        for (action, args) in [
            ("file_operations:open", Some(sentinel)),
            ("file_operations:other", None),
        ] {
            let error = parse_action(&Action {
                label: String::new(),
                desc: String::new(),
                action: action.into(),
                args: args.map(str::to_owned),
            })
            .unwrap_err()
            .to_string();
            assert!(!error.contains(sentinel));
            assert!(error.contains("File Operations Center"));
        }
    }
}
