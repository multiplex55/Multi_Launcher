use super::WidgetSettingsContext;
use crate::actions::Action;

fn collect_query_suggestions(out: &mut Vec<String>, actions: &[Action], prefixes: &[String]) {
    for action in actions {
        let label = action.label.trim();
        let label_lower = label.to_lowercase();
        if prefixes.iter().any(|p| label_lower.starts_with(p)) {
            if !out.iter().any(|s| s.eq_ignore_ascii_case(label)) {
                out.push(label.to_string());
            }
            continue;
        }
        if let Some(query) = action.action.strip_prefix("query:") {
            let q_lower = query.to_lowercase();
            if prefixes.iter().any(|p| q_lower.starts_with(p))
                && !out.iter().any(|s| s.eq_ignore_ascii_case(query))
            {
                out.push(query.to_string());
            }
        }
    }
}

pub(crate) fn query_suggestions(
    ctx: &WidgetSettingsContext<'_>,
    plugin_prefixes: &[&str],
    defaults: &[&str],
) -> Vec<String> {
    let mut out = Vec::new();
    let prefixes: Vec<String> = plugin_prefixes.iter().map(|p| p.to_lowercase()).collect();
    if let Some(cmds) = ctx.plugin_commands {
        collect_query_suggestions(&mut out, cmds, &prefixes);
    }
    if let Some(actions) = ctx.actions {
        collect_query_suggestions(&mut out, actions, &prefixes);
    }
    for def in defaults {
        if !out.iter().any(|s| s.eq_ignore_ascii_case(def)) {
            out.push(def.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::query_suggestions;
    use crate::actions::Action;
    use crate::dashboard::widgets::WidgetSettingsContext;

    fn action(label: &str, action: &str) -> Action {
        Action {
            label: label.into(),
            desc: String::new(),
            action: action.into(),
            args: None,
        }
    }

    #[test]
    fn query_suggestions_dedupes_case_insensitively_and_preserves_priority() {
        let plugin_commands = vec![
            action("todo list", "noop"),
            action("Ignored", "query:todo add"),
            action("TODO LIST", "noop"),
        ];
        let actions = vec![
            action("todo add", "noop"),
            action("other", "query:todo list"),
        ];
        let ctx = WidgetSettingsContext {
            plugin_commands: Some(&plugin_commands),
            actions: Some(&actions),
            ..WidgetSettingsContext::empty()
        };

        let suggestions = query_suggestions(&ctx, &["todo"], &["todo", "todo list", "todo add"]);

        assert_eq!(suggestions, vec!["todo list", "todo add", "todo"]);
    }
}
