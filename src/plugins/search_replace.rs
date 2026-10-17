use crate::actions::Action;
use crate::plugin::Plugin;

pub struct SearchReplacePlugin;

impl Plugin for SearchReplacePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("replace") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Search and Replace".into(),
            desc: "Choose an explicit folder (direct children by default; enable subfolders), then filter supported extensions and root-relative include/exclude paths or globs. Search literal text or Rust regex and select exact matches individually; nothing is selected by default. Apply freezes the reviewed UTF-8 or UTF-8 BOM bytes, rechecks each source, and records durable backups. Stale files are skipped; cancellation can leave a partial batch. Restore applies to committed writes, while Recovery reconciles interrupted operations. Normal fs search is unchanged.".into(),
            action: "replace:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "search_replace"
    }

    fn description(&self) -> &str {
        "Search supported UTF-8 text in an explicit folder, filter the root-relative scope, approve exact matches, then confirm guarded replacement with durable backups, Restore, and interrupted-operation recovery receipts (prefix: `replace`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["replace"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "replace".into(),
            desc: "Choose an explicit folder (direct children by default; enable subfolders) and filter supported extensions plus root-relative include/exclude paths or globs. Search literal text or Rust regex with capture substitutions; preview original and proposed text and select exact matches individually (none are preselected). Apply freezes the reviewed UTF-8 or UTF-8 BOM bytes, rechecks each source, and retains unchanged bytes and line endings. Stale files are skipped; cancellation can leave a partial batch. Durable backups and receipts remain in application data for explicit Restore of committed writes or Recovery of interrupted operations. Normal fs search is unchanged.".into(),
            action: "query:replace".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, SearchReplaceCommand, parse_action};

    #[test]
    fn replace_prefix_opens_a_payload_free_typed_command() {
        for query in ["replace", " REPLACE ", "\trePlace\n"] {
            let actions = SearchReplacePlugin.search(query);
            assert_eq!(actions.len(), 1, "{query:?}");
            assert_eq!(actions[0].action, "replace:open");
            assert!(actions[0].args.is_none());
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::SearchReplace(SearchReplaceCommand::Open)
            );
        }
        for query in ["replace-me", "replace:open", "replace path"] {
            assert!(SearchReplacePlugin.search(query).is_empty(), "{query:?}");
        }
        let mut payload = SearchReplacePlugin.search("replace").remove(0);
        payload.args = Some("untrusted content".into());
        assert!(
            parse_action(&payload)
                .unwrap_err()
                .message
                .contains("does not accept a path or content payload")
        );
        let mut malformed = SearchReplacePlugin.search("replace").remove(0);
        malformed.action = "replace:future".into();
        assert!(
            parse_action(&malformed)
                .unwrap_err()
                .message
                .contains("unknown Search and Replace command")
        );
    }

    #[test]
    fn quick_help_documents_explicit_scope_and_recovery() {
        let command = SearchReplacePlugin
            .commands()
            .into_iter()
            .next()
            .expect("one help command");
        assert_eq!(command.action, "query:replace");
        assert!(command.desc.contains("explicit folder"));
        assert!(command.desc.contains("exact matches"));
        assert!(command.desc.contains("Restore"));
        assert!(command.desc.contains("direct children by default"));
        assert!(command.desc.contains("none are preselected"));
        assert!(command.desc.contains("Normal fs search is unchanged"));
    }
}
