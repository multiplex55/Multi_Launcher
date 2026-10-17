use crate::actions::Action;
use crate::plugin::Plugin;

pub struct XmlToolkitPlugin;

impl Plugin for XmlToolkitPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("xml") {
            return Vec::new();
        }

        vec![Action {
            label: "Open XML Toolkit".into(),
            desc: "Browse, query, validate, format, and save bounded local UTF-8 XML".into(),
            action: "xml:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "xml_toolkit"
    }

    fn description(&self) -> &str {
        "Local UTF-8 XML toolkit with DTD/external resources disabled and root-confined XSDs (prefix: `xml`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["xml"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "xml".into(),
            desc: "Open the bounded offline XML Toolkit".into(),
            action: "query:xml".into(),
            args: None,
        }]
    }
}
