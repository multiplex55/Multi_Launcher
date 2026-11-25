//! Admission of already-published scalar Calculator rows.
use super::RootWindow;
use gpui_kit::Context;
use multi_launcher::actions::Action;
use multi_launcher::commands::{CalculatorCommand, Command, parse_action};

pub(super) const SEARCH_PENDING_STATUS: &str = "Wait for the Calculator search to finish.";

impl RootWindow {
    pub(super) fn start_pending_calculator_search(&mut self, cx: &mut Context<Self>) {
        if self.calculator_search.active {
            return;
        }
        let Some(pending) = self.calculator_search.pending.take() else {
            return;
        };
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        self.calculator_search.active = true;
        let query = pending.query.clone();
        let settings = self.search_catalog.calculator_settings.clone();
        let enabled = self.enabled_plugins.clone();
        let caps = self.enabled_capabilities.clone();
        let task = cx.background_executor().spawn(async move {
            let rows = run_search(&query, settings, enabled, caps);
            (rows, permit)
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (rows, permit) = task.await;
            let _ = owner.update(cx, |root, cx| {
                root.calculator_search.active = false;
                if root.restart_status.as_deref() == Some(SEARCH_PENDING_STATUS) {
                    root.restart_status = None;
                }
                let gate_enabled = root
                    .enabled_plugins
                    .as_ref()
                    .is_none_or(|enabled| enabled.contains("calculator"))
                    && multi_launcher::settings::provider_search_capability_enabled(
                        root.enabled_capabilities.as_ref(),
                        "calculator",
                    );
                let current = publication_is_current(
                    pending.generation,
                    root.launcher_results.generation,
                    &pending.query,
                    &root.launcher_results.query,
                    root.query.read(cx).value().as_str(),
                    gate_enabled,
                    root.restart_admission.is_fenced(),
                );
                if current {
                    let mut actions = pending.other_rows;
                    actions.splice(pending.insert_at..pending.insert_at, rows);
                    root.publish_launcher_provider_results(pending.query, actions, false);
                }
                drop(permit);
                root.start_pending_calculator_search(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

#[derive(Default)]
pub(super) struct SearchOwner {
    pub active: bool,
    pub pending: Option<PendingSearch>,
}

pub(super) struct PendingSearch {
    pub query: String,
    pub generation: u64,
    pub other_rows: Vec<(String, Action)>,
    pub insert_at: usize,
}

pub(super) fn publication_is_current(
    expected_generation: u64,
    current_generation: u64,
    expected_query: &str,
    result_query: &str,
    live_query: &str,
    gate_enabled: bool,
    fenced: bool,
) -> bool {
    expected_generation == current_generation
        && expected_query == result_query
        && expected_query == live_query
        && gate_enabled
        && !fenced
}

pub(super) fn run_search(
    query: &str,
    settings: Option<serde_json::Value>,
    enabled: Option<std::collections::HashSet<String>>,
    caps: Option<std::collections::HashMap<String, Vec<String>>>,
) -> Vec<(String, Action)> {
    use multi_launcher::plugin::Plugin;
    let mut plugin = multi_launcher::plugins_builtin::CalculatorPlugin::default();
    if let Some(settings) = settings {
        plugin.apply_settings(&settings);
    }
    super::search_providers::search_routed(
        [&plugin as &dyn Plugin],
        query,
        enabled.as_ref(),
        caps.as_ref(),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScalarResult {
    pub result: String,
    pub expression: Option<String>,
}

pub(super) fn scalar_expression(query: &str) -> Option<String> {
    let (filtered, _) = multi_launcher::common::query::split_action_filters(query);
    let expression = filtered.trim().strip_prefix('=')?.trim();
    if expression.is_empty()
        || expression.eq_ignore_ascii_case("history")
        || expression.eq_ignore_ascii_case("list")
    {
        return None;
    }
    Some(expression.to_owned())
}

pub(super) fn classify(name: &str, query: &str, action: &Action) -> Option<ScalarResult> {
    if name != "calculator" || action.desc != "Calculator" {
        return None;
    }
    let source = scalar_expression(query)?;
    let Command::Calculator(CalculatorCommand::CopyResult { result, expression }) =
        parse_action(action).ok()?
    else {
        return None;
    };
    if result.is_empty()
        || action.action.strip_prefix("calc:") != Some(result.as_str())
        || expression.as_ref().is_some_and(|value| value != &source)
        || action.label != format!("{source} = {result}")
    {
        return None;
    }
    Some(ScalarResult { result, expression })
}

#[cfg(test)]
mod tests {
    use super::{classify, scalar_expression};
    use multi_launcher::actions::Action;

    #[test]
    fn save_on_enter_stock_search_publishes_exact_expression_without_copy() {
        let enabled = std::collections::HashSet::from(["calculator".to_owned()]);
        let query = "= 6 * 7";
        let rows = super::run_search(
            query,
            Some(serde_json::json!({"save_on_enter": true})),
            Some(enabled),
            None,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.action, "calc:42");
        assert_eq!(rows[0].1.args.as_deref(), Some("6 * 7"));
        assert_eq!(
            super::classify(&rows[0].0, query, &rows[0].1)
                .unwrap()
                .result,
            "42"
        );
    }

    #[test]
    fn calculator_publication_rejects_stale_generation_query_and_gates() {
        use super::publication_is_current;
        assert!(publication_is_current(
            3, 3, "= 2+2", "= 2+2", "= 2+2", true, false
        ));
        assert!(!publication_is_current(
            3, 4, "= 2+2", "= 2+2", "= 2+2", true, false
        ));
        assert!(!publication_is_current(
            3, 3, "= 2+2", "= 2+2", "= 3+1", true, false
        ));
        assert!(!publication_is_current(
            3, 3, "= 2+2", "= 2+2", "= 2+2", false, false
        ));
        assert!(!publication_is_current(
            3, 3, "= 2+2", "= 2+2", "= 2+2", true, true
        ));
    }

    #[test]
    fn scalar_result_requires_current_expression_and_typed_origin() {
        let action = Action {
            label: "2 + 2 = 4".into(),
            desc: "Calculator".into(),
            action: "calc:4".into(),
            args: Some("2 + 2".into()),
        };
        assert!(classify("calculator", "= 2 + 2", &action).is_some());
        assert!(classify("other", "= 2 + 2", &action).is_none());
        assert!(classify("calculator", "= 3 + 1", &action).is_none());
        assert!(scalar_expression("= history").is_none());
        assert!(scalar_expression("= list").is_none());
        assert!(scalar_expression("= ").is_none());
    }
}
