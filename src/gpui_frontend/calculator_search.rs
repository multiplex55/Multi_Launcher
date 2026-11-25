//! Admission of already-published Calculator rows.
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
            let output = run_search(&query, settings, enabled, caps);
            (output, permit)
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (output, permit) = task.await;
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
                    actions.splice(pending.insert_at..pending.insert_at, output.rows);
                    root.publish_launcher_provider_results(pending.query, actions, false);
                    root.calculator_search.history = output.history.map(|entries| HistoryReceipt {
                        generation: root.launcher_results.generation,
                        entries,
                    });
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
    history: Option<HistoryReceipt>,
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

type HistoryEntry = multi_launcher::plugins::calc_history::CalcHistoryEntry;

struct HistoryReceipt {
    generation: u64,
    entries: Vec<HistoryEntry>,
}

pub(super) struct SearchOutput {
    pub rows: Vec<(String, Action)>,
    history: Option<Vec<HistoryEntry>>,
}

pub(super) fn history_query(query: &str) -> bool {
    let (filtered, _) = multi_launcher::common::query::split_action_filters(query);
    multi_launcher::plugins_builtin::calculator_history_query(&filtered)
}

pub(super) fn eligible_query(query: &str) -> bool {
    history_query(query) || scalar_expression(query).is_some()
}

struct HistoryPlugin<'a>(&'a [HistoryEntry]);
impl multi_launcher::plugin::Plugin for HistoryPlugin<'_> {
    fn name(&self) -> &str { "calculator" }
    fn description(&self) -> &str { "Calculator" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn search(&self, query: &str) -> Vec<Action> {
        if history_query(query) { multi_launcher::plugins_builtin::calculator_history_rows(self.0) }
        else { Vec::new() }
    }
}

pub(super) fn run_search(
    query: &str,
    settings: Option<serde_json::Value>,
    enabled: Option<std::collections::HashSet<String>>,
    caps: Option<std::collections::HashMap<String, Vec<String>>>,
) -> SearchOutput {
    use multi_launcher::plugin::Plugin;
    if history_query(query) {
        let entries: Vec<_> = multi_launcher::plugins::calc_history::load_history(
            multi_launcher::plugins::calc_history::CALC_HISTORY_FILE,
        ).unwrap_or_default().into_iter().collect();
        let rows = super::search_providers::search_routed(
            [&HistoryPlugin(&entries) as &dyn Plugin], query, enabled.as_ref(), caps.as_ref(),
        );
        SearchOutput { rows, history: Some(entries) }
    } else {
        let mut plugin = multi_launcher::plugins_builtin::CalculatorPlugin::default();
        if let Some(settings) = settings { plugin.apply_settings(&settings); }
        let rows = super::search_providers::search_routed(
            [&plugin as &dyn Plugin], query, enabled.as_ref(), caps.as_ref(),
        );
        SearchOutput { rows, history: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HistoryResult {
    pub index: usize,
    pub entry: HistoryEntry,
}

pub(super) fn history_index(name: &str, query: &str, action: &Action) -> Option<usize> {
    if name != "calculator" || !history_query(query) || action.desc != "Calculator" || action.args.is_some() {
        return None;
    }
    let Command::Calculator(CalculatorCommand::CopyHistory { index }) = parse_action(action).ok()? else { return None; };
    (action.action == format!("calc:history:{index}")).then_some(index)
}

impl SearchOwner {
    pub(super) fn retire_history(&mut self) { self.history = None; }

    pub(super) fn history_result(&self, generation: u64, query: &str, action: &Action) -> Option<HistoryResult> {
        let index = history_index("calculator", query, action)?;
        let receipt = self.history.as_ref().filter(|receipt| receipt.generation == generation)?;
        let entry = receipt.entries.get(index)?;
        (multi_launcher::plugins_builtin::calculator_history_row(index, entry) == *action)
            .then(|| HistoryResult { index, entry: entry.clone() })
    }
}

pub(super) fn copy_checked_history_with(
    result: &HistoryResult,
    read: impl FnOnce() -> anyhow::Result<std::collections::VecDeque<HistoryEntry>>,
    copy: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let entries = read()?;
    let current = entries.get(result.index).filter(|entry| *entry == &result.entry)
        .ok_or_else(|| anyhow::anyhow!("The Calculator history result is stale; refresh and try again"))?;
    #[cfg(windows)]
    anyhow::ensure!(
        !current.result.contains('\0'),
        "The Calculator history value contains a NUL character and cannot be copied exactly"
    );
    copy(&current.result)
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
        let output = super::run_search(
            query,
            Some(serde_json::json!({"save_on_enter": true})),
            Some(enabled),
            None,
        );
        let rows = output.rows;
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

    #[test]
    fn history_aliases_preserve_filtered_wire_index_and_receipt_generation() {
        use super::{HistoryEntry, HistoryPlugin, HistoryReceipt, SearchOwner, history_query};
        use multi_launcher::plugin::Plugin;
        let entries = vec![
            HistoryEntry { expr: "first".into(), result: "1".into() },
            HistoryEntry { expr: "second".into(), result: "2".into() },
        ];
        let plugin = HistoryPlugin(&entries);
        for query in ["calc list", "= LIST", "= History", "id:calc:history:1 = list"] {
            assert!(history_query(query));
        }
        let rows = super::super::search_providers::search_routed(
            [&plugin as &dyn Plugin], "id:calc:history:1 = list", None, None,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.action, "calc:history:1");
        let owner = SearchOwner { active: false, pending: None,
            history: Some(HistoryReceipt { generation: 9, entries }) };
        let result = owner.history_result(9, "id:calc:history:1 = list", &rows[0].1).unwrap();
        assert_eq!(result.index, 1);
        assert_eq!(result.entry.expr, "second");
        assert!(owner.history_result(10, "= list", &rows[0].1).is_none());
    }

    #[test]
    fn history_copy_rechecks_entire_entry_and_never_writes() {
        use super::{HistoryEntry, HistoryResult, copy_checked_history_with};
        use std::cell::RefCell;
        use std::collections::VecDeque;
        let entry = HistoryEntry { expr: "a = b".into(), result: "c".into() };
        let result = HistoryResult { index: 0, entry: entry.clone() };
        let copied = RefCell::new(Vec::new());
        copy_checked_history_with(&result, || Ok(VecDeque::from([entry.clone()])), |value| {
            copied.borrow_mut().push(value.to_owned()); Ok(())
        }).unwrap();
        assert_eq!(*copied.borrow(), ["c"]);
        copied.borrow_mut().clear();
        // The alternate entry renders the same label while representing different data.
        let different = HistoryEntry { expr: "a".into(), result: "b = c".into() };
        assert!(copy_checked_history_with(&result, || Ok(VecDeque::from([different])), |_| {
            panic!("stale entry must not copy")
        }).is_err());
        assert!(copy_checked_history_with(&result, || anyhow::bail!("read failed"), |_| {
            panic!("failed read must not copy")
        }).is_err());
        assert!(copy_checked_history_with(&result, || Ok(VecDeque::from([entry])), |_| {
            anyhow::bail!("clipboard failed")
        }).is_err());
        assert!(copied.borrow().is_empty());
        #[cfg(windows)]
        {
            let nul_entry = HistoryEntry { expr: "nul".into(), result: "a\0b".into() };
            let nul_result = HistoryResult { index: 0, entry: nul_entry.clone() };
            assert!(copy_checked_history_with(&nul_result, || Ok(VecDeque::from([nul_entry])), |_| {
                panic!("NUL value must not reach the clipboard")
            }).is_err());
        }
    }

}
