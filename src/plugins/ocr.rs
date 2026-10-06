use crate::actions::Action;
use crate::plugin::Plugin;

/// A normal launcher utility backed by the shared local OCR workflow.
pub struct OcrPlugin;

fn start_action() -> Action {
    Action {
        label: "OCR Screen Region".into(),
        desc: "Select a screen region and recognize English text locally".into(),
        action: "ocr:start".into(),
        args: None,
    }
}

impl Plugin for OcrPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("ocr") {
            vec![start_action()]
        } else {
            Vec::new()
        }
    }
    fn name(&self) -> &str {
        "ocr"
    }
    fn description(&self) -> &str {
        "Local English screen-region OCR (prefix: `ocr`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["ocr"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![start_action()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ocr_search_accepts_only_exact_query_and_exposes_assignable_command() {
        let plugin = OcrPlugin;
        let command = plugin.commands();
        assert_eq!(command.len(), 1);
        assert_eq!(command[0].action, "ocr:start");
        for query in ["ocr", "OCR", "OcR", "  ocr \t\n"] {
            assert_eq!(plugin.search(query), command, "{query:?}");
        }
        for query in [
            "",
            "oc",
            "ocra",
            "ocrcapture",
            "ocr start",
            "ocr: start",
            "ocr:start",
            "other ocr",
        ] {
            assert!(plugin.search(query).is_empty(), "{query:?}");
        }
    }
}
