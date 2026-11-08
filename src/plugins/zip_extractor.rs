use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ZipExtractorPlugin;

impl Plugin for ZipExtractorPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("zip") {
            return Vec::new();
        }
        vec![Action {
            label: "Open ZIP Extractor".into(),
            desc: "Create ZIPs or preview and extract Stored and Deflated entries".into(),
            action: "zip:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "zip_extractor"
    }

    fn description(&self) -> &str {
        "Create ZIPs or preview and extract Stored and Deflated entries (prefix: `zip`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["zip"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "zip".into(),
            desc: "Open the ZIP Extractor".into(),
            action: "query:zip".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, ZipExtractorCommand, parse_action};

    #[test]
    fn zip_prefix_is_an_exact_payload_free_action() {
        let plugin = ZipExtractorPlugin;
        let action = plugin.search(" ZIP ");
        assert_eq!(action.len(), 1);
        assert_eq!(action[0].action, "zip:open");
        assert_eq!(action[0].args, None);
        assert_eq!(
            parse_action(&action[0]).unwrap(),
            Command::ZipExtractor(ZipExtractorCommand::Open)
        );
        for query in ["zip path.zip", "zip:open", "zip extract", "zipper"] {
            assert!(plugin.search(query).is_empty(), "{query:?}");
        }
        assert_eq!(plugin.query_prefixes(), &["zip"]);
        assert_eq!(plugin.commands()[0].action, "query:zip");
    }
}
