use crate::actions::Action;
use crate::plugin::Plugin;

pub struct FolderSizePlugin;

impl Plugin for FolderSizePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("foldersize") {
            return Vec::new();
        }

        vec![Action {
            label: "Open Folder Size".into(),
            desc: "Measure logical file sizes under a folder you choose".into(),
            action: "foldersize:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "folder_size"
    }

    fn description(&self) -> &str {
        "Read-only logical folder-size analysis for explicitly selected folders (query: `foldersize`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["foldersize"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "foldersize".into(),
            desc: "Open Folder Size".into(),
            action: "query:foldersize".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_size_command_plugin_is_exact_and_routes_to_the_typed_dialog() {
        let plugin = FolderSizePlugin;
        assert!(plugin.search("folder size").is_empty());
        assert!(plugin.search("foldersize extra").is_empty());

        let matches = plugin.search("  FolderSize  ");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].action, "foldersize:open");
        assert_eq!(
            crate::commands::parse_action(&matches[0]).unwrap(),
            crate::commands::Command::Dialog(crate::commands::DialogCommand::FolderSize)
        );
        assert_eq!(
            plugin.commands()[0].action,
            "query:foldersize",
            "the command picker exposes the same exact query prefix"
        );
    }
}
