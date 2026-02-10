use crate::actions::Action;
use crate::linking::{
    resolve_link, EntityKey, LinkResolverCatalog, LinkTarget, TracingLinkTelemetry,
};
use crate::plugin::Plugin;
use crate::plugins::note::{load_notes, Note};
use crate::plugins::todo::load_todos;

pub struct LinkPlugin;

impl LinkPlugin {
    fn build_catalog(
        notes: &[Note],
        todos: &[crate::plugins::todo::TodoEntry],
    ) -> LinkResolverCatalog {
        let mut catalog = LinkResolverCatalog::default();
        for n in notes {
            let key = EntityKey::new(LinkTarget::Note, n.slug.clone());
            catalog.add_target(key.clone());
            for line in n.content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix('#') {
                    let anchor = rest.trim().to_ascii_lowercase().replace(' ', "-");
                    if !anchor.is_empty() {
                        catalog.add_anchor(key.clone(), anchor);
                    }
                }
            }
        }
        for t in todos {
            catalog.add_target(EntityKey::new(LinkTarget::Todo, t.id.clone()));
        }
        catalog
    }
}

impl Plugin for LinkPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim_start();
        if !q.eq_ignore_ascii_case("link") && !q.to_ascii_lowercase().starts_with("link ") {
            return Vec::new();
        }
        let rest = q.strip_prefix("link").unwrap_or("").trim();
        if rest.is_empty() {
            return vec![Action {
                label: "Usage: link <id>".into(),
                desc: "Link".into(),
                action: "query:link ".into(),
                args: None,
            }];
        }
        let notes = load_notes().unwrap_or_default();
        let todos = load_todos(crate::plugins::todo::TODO_FILE).unwrap_or_default();
        let catalog = Self::build_catalog(&notes, &todos);
        let telemetry = TracingLinkTelemetry;
        match resolve_link(rest, &catalog, &telemetry) {
            Ok(resolved) => vec![Action {
                label: format!("Open {}", resolved.location),
                desc: "Link".into(),
                action: format!("link:open:{}", resolved.location),
                args: None,
            }],
            Err(_) => vec![Action {
                label: format!("Invalid or broken link id: {rest}"),
                desc: "Link".into(),
                action: "query:link ".into(),
                args: None,
            }],
        }
    }

    fn name(&self) -> &str {
        "link"
    }

    fn description(&self) -> &str {
        "Resolve canonical link IDs (prefix: `link`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "link".into(),
            desc: "Link".into(),
            action: "query:link ".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_parsing_usage() {
        let plugin = LinkPlugin;
        let res = plugin.search("link");
        assert_eq!(res[0].label, "Usage: link <id>");
    }
}
