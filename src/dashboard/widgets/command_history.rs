use super::{
    BackgroundLoader, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    edit_typed_settings,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::history::{HISTORY_PINS_FILE, HistoryEntry, HistoryPin, toggle_pin};
use chrono::TimeZone;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

fn default_count() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistoryConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default)]
    pub show_pinned_only: bool,
    #[serde(default = "default_show_filter")]
    pub show_filter: bool,
}

impl Default for CommandHistoryConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_pinned_only: false,
            show_filter: default_show_filter(),
        }
    }
}

fn default_show_filter() -> bool {
    true
}

#[derive(Clone)]
struct DisplayEntry {
    action_id: String,
    action: Action,
    query: String,
    timestamp: i64,
    pinned: bool,
    missing: bool,
}

pub struct CommandHistoryWidget {
    cfg: CommandHistoryConfig,
    filter: String,
    cached_pins: Vec<HistoryPin>,
    pins_loader: BackgroundLoader<(), anyhow::Result<Vec<HistoryPin>>>,
    last_pins_load: Instant,
}

struct HistoryResolutionContext<'a> {
    snapshot: &'a crate::dashboard::data_cache::DashboardDataSnapshot,
    commands: &'a [Action],
    actions_by_id: &'a std::collections::HashMap<String, Action>,
}

impl CommandHistoryWidget {
    pub fn new(cfg: CommandHistoryConfig) -> Self {
        Self {
            cfg,
            filter: String::new(),
            cached_pins: Vec::new(),
            pins_loader: BackgroundLoader::new(|()| crate::history::load_pins(HISTORY_PINS_FILE)),
            last_pins_load: Instant::now() - Duration::from_secs(10),
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(
            ui,
            value,
            ctx,
            |ui, cfg: &mut CommandHistoryConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Count");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                        .changed();
                });
                changed |= ui
                    .checkbox(&mut cfg.show_pinned_only, "Show pinned only")
                    .changed();
                changed |= ui.checkbox(&mut cfg.show_filter, "Show filter").changed();
                changed
            },
        )
    }

    fn refresh_pins(&mut self, repaint: &egui::Context) {
        if let Some(result) = self.pins_loader.poll() {
            publish_pins_or_retain(&mut self.cached_pins, result);
        }
        if self.last_pins_load.elapsed() > Duration::from_secs(2) {
            if self.pins_loader.request((), repaint) {
                self.last_pins_load = Instant::now();
            }
        }
    }

    fn format_timestamp(ts: i64) -> String {
        if ts <= 0 {
            return "Unknown time".into();
        }
        chrono::Local
            .timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Unknown time".into())
    }

    fn entry_matches_filter(entry: &DisplayEntry, filter: &str) -> bool {
        if filter.is_empty() {
            return true;
        }
        let filter = filter.to_lowercase();
        entry.action.label.to_lowercase().contains(&filter)
            || entry.query.to_lowercase().contains(&filter)
    }

    fn resolve_action(
        ctx: &HistoryResolutionContext<'_>,
        action_id: &str,
        args: Option<&str>,
        saved_action: &Action,
    ) -> Option<Action> {
        let _timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::HistoryResolve);
        if action_id.starts_with("snippet:run:") {
            return crate::plugins::snippets::resolve_snippet_run_action_from_entries(
                action_id,
                args,
                &ctx.snapshot.snippets,
            );
        }

        if let Some(action) = ctx.actions_by_id.get(action_id) {
            return Some(action.clone());
        }

        if let Some(action) = ctx
            .commands
            .iter()
            .find(|action| action.action == action_id && action.args.as_deref() == args)
        {
            return Some(action.clone());
        }

        if let Some(action) = ctx
            .snapshot
            .processes
            .iter()
            .find(|action| action.action == action_id && action.args.as_deref() == args)
        {
            return Some(action.clone());
        }

        if let Some(fav) = ctx
            .snapshot
            .favorites
            .iter()
            .find(|fav| fav.action == action_id && fav.args.as_deref() == args)
        {
            return Some(Action {
                label: fav.label.clone(),
                desc: "Fav".into(),
                action: fav.action.clone(),
                args: fav.args.clone(),
            });
        }

        if let Some(slug) = action_id.strip_prefix("note:open:")
            && let Some(note) = ctx.snapshot.notes.iter().find(|note| note.slug == slug)
        {
            return Some(Action {
                label: note.alias.as_ref().unwrap_or(&note.title).clone(),
                desc: "Note".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(idx) = action_id
            .strip_prefix("clipboard:copy:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(entry) = ctx.snapshot.clipboard_history.get(idx)
        {
            return Some(Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(idx) = action_id
            .strip_prefix("todo:done:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = ctx.snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                desc: "Todo".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(idx) = action_id
            .strip_prefix("todo:edit:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = ctx.snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                desc: "Todo".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(idx) = action_id
            .strip_prefix("todo:remove:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = ctx.snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("Remove todo {}", todo.text),
                desc: "Todo".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(alias) = action_id.strip_prefix("snippet:edit:")
            && ctx.snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Edit snippet {alias}"),
                desc: "Snippet".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(alias) = action_id.strip_prefix("snippet:remove:")
            && ctx.snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Remove snippet {alias}"),
                desc: "Snippet".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        // Old snippets were stored as literal clipboard commands. Keep those
        // opaque actions runnable and retain their saved presentation, while
        // indexed clipboard-history actions above remain availability-checked.
        if action_id.starts_with("clipboard:")
            && !action_id.starts_with("clipboard:copy:")
            && saved_action.action == action_id
            && saved_action.args.as_deref() == args
        {
            return Some(saved_action.clone());
        }

        None
    }

    fn entry_from_history(
        ctx: &HistoryResolutionContext<'_>,
        entry: &HistoryEntry,
    ) -> DisplayEntry {
        let resolved = Self::resolve_action(
            ctx,
            &entry.action.action,
            entry.action.args.as_deref(),
            &entry.action,
        );
        let action = resolved.unwrap_or_else(|| entry.action.clone());
        DisplayEntry {
            action_id: entry.action.action.clone(),
            action,
            query: entry.query.clone(),
            timestamp: entry.timestamp,
            pinned: false,
            missing: false,
        }
    }

    fn entry_from_pin(ctx: &HistoryResolutionContext<'_>, pin: &HistoryPin) -> DisplayEntry {
        let fallback = Action {
            label: pin.label.clone(),
            desc: pin.desc.clone(),
            action: pin.action_id.clone(),
            args: pin.args.clone(),
        };
        let resolved = Self::resolve_action(ctx, &pin.action_id, pin.args.as_deref(), &fallback);
        let action = resolved.clone().unwrap_or(fallback);
        DisplayEntry {
            action_id: pin.action_id.clone(),
            action,
            query: pin.query.clone(),
            timestamp: pin.timestamp,
            pinned: true,
            missing: resolved.is_none(),
        }
    }

    fn is_pinned(pins: &[HistoryPin], entry: &HistoryEntry) -> bool {
        let pin = HistoryPin::from_history(entry);
        pins.iter().any(|p| p == &pin)
    }

    fn prepare_entries(&self, ctx: &DashboardContext<'_>) -> Vec<DisplayEntry> {
        let mut prepare_timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::HistoryPrepare);
        prepare_timer.set_work_units(0);
        let history_entries =
            crate::history::with_history(|h| h.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
        prepare_timer.set_work_units(history_entries.len() as u64);

        let snapshot = ctx.data_cache.snapshot();
        let commands = {
            let mut catalog_timer = crate::performance::MetricTimer::start(
                crate::performance::Metric::HistoryCatalogBuild,
            );
            catalog_timer.set_work_units(0);
            let commands = ctx.plugins.commands_filtered(ctx.enabled_plugins);
            catalog_timer.set_work_units(commands.len() as u64);
            commands
        };
        let resolution = HistoryResolutionContext {
            snapshot: &snapshot,
            commands: &commands,
            actions_by_id: ctx.actions_by_id,
        };

        let mut entries: Vec<DisplayEntry> = Vec::new();
        if self.cfg.show_pinned_only {
            entries.extend(
                self.cached_pins
                    .iter()
                    .map(|pin| Self::entry_from_pin(&resolution, pin)),
            );
        } else {
            let mut pinned: Vec<DisplayEntry> = self
                .cached_pins
                .iter()
                .map(|pin| Self::entry_from_pin(&resolution, pin))
                .collect();
            pinned.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
            entries.extend(pinned);

            for entry in &history_entries {
                if Self::is_pinned(&self.cached_pins, entry) {
                    continue;
                }
                entries.push(Self::entry_from_history(&resolution, entry));
            }
        }

        let filtered = entries
            .into_iter()
            .filter(|entry| Self::entry_matches_filter(entry, &self.filter))
            .take(self.cfg.count)
            .collect::<Vec<_>>();
        drop(prepare_timer);
        filtered
    }
}

impl Default for CommandHistoryWidget {
    fn default() -> Self {
        Self::new(CommandHistoryConfig::default())
    }
}

impl Widget for CommandHistoryWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_pins(ui.ctx());
        let mut clicked = None;
        ui.label("Command history");

        if self.cfg.show_filter {
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.filter);
            });
        }

        let filtered = self.prepare_entries(ctx);

        if filtered.is_empty() {
            ui.label("No history entries.");
        }

        for entry in filtered {
            let timestamp = Self::format_timestamp(entry.timestamp);
            ui.horizontal(|ui| {
                let pin_label = if entry.pinned { "★" } else { "☆" };
                if ui.button(pin_label).clicked() {
                    let pin = HistoryPin {
                        action_id: entry.action_id.clone(),
                        label: entry.action.label.clone(),
                        desc: entry.action.desc.clone(),
                        args: entry.action.args.clone(),
                        query: entry.query.clone(),
                        timestamp: entry.timestamp,
                    };
                    if let Ok(pinned) = toggle_pin(HISTORY_PINS_FILE, &pin) {
                        if pinned {
                            self.cached_pins.push(pin);
                        } else {
                            self.cached_pins.retain(|p| p != &pin);
                        }
                    }
                }

                let action_label = if entry.missing {
                    format!("{} (missing)", entry.action.label)
                } else {
                    entry.action.label.clone()
                };
                if entry.missing {
                    ui.colored_label(egui::Color32::YELLOW, action_label);
                } else if ui.button(&action_label).clicked() {
                    clicked = Some(WidgetAction {
                        action: entry.action.clone(),
                        query_override: Some(entry.query.clone()),
                    });
                }
                if entry.missing && ui.button("Unpin").clicked() {
                    if matches!(
                        crate::history::remove_pin(
                            HISTORY_PINS_FILE,
                            &entry.action_id,
                            entry.action.args.as_deref(),
                        ),
                        Ok(true)
                    ) {
                        self.cached_pins.retain(|p| {
                            p.action_id != entry.action_id || p.args != entry.action.args
                        });
                    }
                }
                ui.label(timestamp);
            });
        }

        clicked
    }
}

fn publish_pins_or_retain(current: &mut Vec<HistoryPin>, result: anyhow::Result<Vec<HistoryPin>>) {
    match result {
        Ok(pins) => *current = pins,
        Err(error) => tracing::error!(%error, "dashboard retained last-good history pins"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::data_cache::{DashboardDataCache, DashboardDataSnapshot};
    use crate::performance::{Metric, workloads};
    use crate::plugin::{Plugin, PluginManager};
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, RwLock};

    fn context<'a>(
        data_cache: &'a DashboardDataCache,
        plugins: &'a PluginManager,
        actions: &'a [Action],
        actions_by_id: &'a HashMap<String, Action>,
        usage: &'a HashMap<String, u32>,
    ) -> DashboardContext<'a> {
        context_with_enabled(data_cache, plugins, actions, actions_by_id, usage, None)
    }

    fn context_with_enabled<'a>(
        data_cache: &'a DashboardDataCache,
        plugins: &'a PluginManager,
        actions: &'a [Action],
        actions_by_id: &'a HashMap<String, Action>,
        usage: &'a HashMap<String, u32>,
        enabled_plugins: Option<&'a HashSet<String>>,
    ) -> DashboardContext<'a> {
        DashboardContext {
            actions,
            actions_by_id,
            usage,
            plugins,
            enabled_plugins,
            default_location: None,
            data_cache,
            actions_version: 0,
            fav_version: 0,
            notes_version: 0,
            todo_version: 0,
            calendar_version: 0,
            clipboard_version: 0,
            snippets_version: 0,
            dashboard_visible: true,
            dashboard_focused: true,
            reduce_dashboard_work_when_unfocused: false,
            diagnostics: None,
            show_diagnostics_widget: false,
        }
    }

    fn resolution_context<'a>(
        snapshot: &'a DashboardDataSnapshot,
        commands: &'a [Action],
        actions_by_id: &'a HashMap<String, Action>,
    ) -> HistoryResolutionContext<'a> {
        HistoryResolutionContext {
            snapshot,
            commands,
            actions_by_id,
        }
    }

    fn action(label: &str, action_id: &str, args: Option<&str>) -> Action {
        Action {
            label: label.into(),
            desc: "fixture".into(),
            action: action_id.into(),
            args: args.map(str::to_owned),
        }
    }

    fn history_entry(action: Action, index: usize) -> HistoryEntry {
        let query = format!("history query {index}");
        HistoryEntry {
            query: query.clone(),
            query_lc: query.to_lowercase(),
            action,
            source: Some("history_resolution_fixture".into()),
            timestamp: 1_700_000_000 + index as i64,
        }
    }

    struct CatalogFixturePlugin(Action);

    impl Plugin for CatalogFixturePlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            Vec::new()
        }

        fn name(&self) -> &str {
            "track_a_history_fixture"
        }

        fn description(&self) -> &str {
            "Synthetic history benchmark command catalog"
        }

        fn capabilities(&self) -> &[&str] {
            &[]
        }

        fn commands(&self) -> Vec<Action> {
            vec![self.0.clone()]
        }
    }

    struct CountingCatalogPlugin {
        name: &'static str,
        actions: Arc<RwLock<Vec<Action>>>,
        calls: Arc<AtomicUsize>,
    }

    impl Plugin for CountingCatalogPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            Vec::new()
        }

        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "Synthetic history command catalog"
        }

        fn capabilities(&self) -> &[&str] {
            &[]
        }

        fn commands(&self) -> Vec<Action> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.actions
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    fn assert_history_scenario(
        scenario: &str,
        entries: &[DisplayEntry],
        fixture: &workloads::HistoryFixture,
    ) {
        match scenario {
            "mixed-count-8-no-filter" => {
                let pin_count = entries.iter().take_while(|entry| entry.pinned).count();
                assert_eq!(pin_count, 5);
                assert_eq!(entries.len(), 8);
                assert!(entries[pin_count..].iter().all(|entry| !entry.pinned));
                assert_eq!(
                    entries[..pin_count]
                        .iter()
                        .map(|entry| entry.action_id.as_str())
                        .collect::<Vec<_>>(),
                    fixture.pins[..pin_count]
                        .iter()
                        .map(|pin| pin.action_id.as_str())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    entries[pin_count].action_id, fixture.entries[2].action.action,
                    "entry 1 shares the (action ID, args) identity of the later catalog pin"
                );
                let catalog_pin = entries
                    .iter()
                    .find(|entry| entry.action_id == fixture.catalog_action.action)
                    .expect("the pinned catalog command is retained");
                assert!(catalog_pin.pinned);
                assert_eq!(catalog_pin.action.label, fixture.catalog_action.label);
            }
            "pins-only-8" => {
                assert_eq!(entries.len(), 8);
                assert!(entries.iter().all(|entry| entry.pinned));
                assert_eq!(
                    entries
                        .iter()
                        .map(|entry| entry.action_id.as_str())
                        .collect::<Vec<_>>(),
                    fixture
                        .pins
                        .iter()
                        .take(8)
                        .map(|pin| pin.action_id.as_str())
                        .collect::<Vec<_>>()
                );
            }
            "mixed-count-8-rare-filter" => {
                assert_eq!(entries.len(), 1);
                let expected = &fixture.entries[fixture.rare_entry_index];
                assert_eq!(entries[0].action_id, expected.action.action);
                assert_eq!(entries[0].query, "rare-query-synthetic");
                assert_eq!(entries[0].timestamp, expected.timestamp);
                assert!(!entries[0].pinned);
            }
            "mixed-count-50-renamed-missing" => {
                assert_eq!(entries.len(), 50);
                let renamed = entries
                    .iter()
                    .find(|entry| fixture.actions_by_id.contains_key(&entry.action_id))
                    .expect("a direct target is visible");
                assert_eq!(
                    renamed.action.label,
                    fixture.actions_by_id[&renamed.action_id].label
                );
                let missing = entries
                    .iter()
                    .find(|entry| entry.pinned && entry.missing)
                    .expect("a missing pinned target is represented");
                assert!(
                    fixture.actions_by_id.get(&missing.action_id).is_none()
                        && missing.action.action == missing.action_id
                );
            }
            _ => unreachable!("unknown Track A history scenario"),
        }
    }

    fn history_output_signature(entries: &[DisplayEntry]) -> u64 {
        let mut signature = workloads::StableSignature::new(0, "history-output", entries.len());
        for entry in entries {
            signature.bytes(entry.action_id.as_bytes());
            signature.bytes(entry.action.args.as_deref().unwrap_or_default().as_bytes());
            signature.bytes(entry.action.label.as_bytes());
            signature.bytes(entry.action.desc.as_bytes());
            signature.bytes(entry.query.as_bytes());
            signature.number(entry.timestamp as u64);
            signature.number(u64::from(entry.pinned));
            signature.number(u64::from(entry.missing));
        }
        signature.finish()
    }

    #[test]
    #[ignore = "opt-in Track A workload benchmark; set MULTI_LAUNCHER_PERF=1 before the process"]
    fn track_a_benchmark_history_prepare_owner() {
        let workspace = workloads::IsolatedWorkspace::new();
        std::fs::write(workspace.root().join("history_pins.json"), b"[]")
            .expect("write isolated pin state");
        let cache = DashboardDataCache::new();
        cache.set_snapshot_for_test(DashboardDataSnapshot::default());
        let usage = HashMap::new();

        for count in workloads::selected_sizes(&[100, 1_000, 10_000]) {
            let fixture = workloads::history_fixture(0x4849_5354_4f52_59, count);
            let _history_guard = crate::history::replace_history_for_test(fixture.entries.clone());
            let mut plugins = PluginManager::new_inert_for_test();
            plugins.register(Box::new(CatalogFixturePlugin(
                fixture.catalog_action.clone(),
            )));
            let actions = Vec::new();
            let ctx = context(&cache, &plugins, &actions, &fixture.actions_by_id, &usage);

            for scenario in [
                "mixed-count-8-no-filter",
                "pins-only-8",
                "mixed-count-8-rare-filter",
                "mixed-count-50-renamed-missing",
            ] {
                let mut widget = CommandHistoryWidget::new(CommandHistoryConfig {
                    count: if scenario == "mixed-count-50-renamed-missing" {
                        50
                    } else {
                        8
                    },
                    show_pinned_only: scenario == "pins-only-8",
                    show_filter: true,
                });
                widget.cached_pins = if scenario == "mixed-count-8-no-filter" {
                    fixture.pins.iter().take(5).cloned().collect()
                } else {
                    fixture.pins.clone()
                };
                widget.filter = if scenario == "mixed-count-8-rare-filter" {
                    "rare-query-synthetic".into()
                } else {
                    String::new()
                };
                let initial = widget.prepare_entries(&ctx);
                assert_history_scenario(scenario, &initial, &fixture);

                let (timing, final_entries) = workloads::measure(
                    workloads::UI_WARMUPS,
                    |_, _| {},
                    || widget.prepare_entries(&ctx),
                );
                let metrics = workloads::metrics_for(&[
                    Metric::HistoryPrepare,
                    Metric::HistoryResolve,
                    Metric::HistoryCatalogBuild,
                ]);
                let catalog_metric = metrics
                    .iter()
                    .find(|metric| metric.metric == Metric::HistoryCatalogBuild)
                    .expect("history command catalog metric is reported");
                assert_eq!(catalog_metric.calls, workloads::SAMPLE_COUNT as u64);
                assert_eq!(catalog_metric.work_units, workloads::SAMPLE_COUNT as u64);
                assert_history_scenario(scenario, &final_entries, &fixture);
                let summary = fixture
                    .summary
                    .with_output_signatures(Some(history_output_signature(&final_entries)), None);
                workloads::emit_summary(
                    &format!("history-{count}-{scenario}"),
                    "headless production history preparation; debug-test CPU, no UI",
                    summary,
                    timing,
                    &metrics,
                );
            }
            drop(plugins);
        }
        drop(cache);
        drop(workspace);
    }

    fn pin(action_id: &str) -> HistoryPin {
        HistoryPin {
            action_id: action_id.into(),
            label: action_id.into(),
            desc: String::new(),
            args: None,
            query: action_id.into(),
            timestamp: 0,
        }
    }

    #[test]
    fn failed_pin_reload_retains_last_good_then_recovers() {
        let initial = vec![pin("saved")];
        let mut current = initial.clone();
        publish_pins_or_retain(&mut current, Err(anyhow::anyhow!("invalid pins")));
        assert_eq!(current, initial);
        let recovered = vec![pin("recovered")];
        publish_pins_or_retain(&mut current, Ok(recovered.clone()));
        assert_eq!(current, recovered);
    }

    #[test]
    fn history_pins_keep_opaque_clipboard_literals_and_resolve_snippets_by_alias() {
        let data_cache = DashboardDataCache::new();
        let mut snapshot = DashboardDataSnapshot::default();
        snapshot.snippets = std::sync::Arc::new(vec![
            crate::plugins::snippets::SnippetEntry {
                alias: "first".into(),
                text: "same body {{literal}}".into(),
                hide_contents: false,
                prompt_for_fields: false,
                fields: Vec::new(),
            },
            crate::plugins::snippets::SnippetEntry {
                alias: "second".into(),
                text: "same body {{literal}}".into(),
                hide_contents: true,
                prompt_for_fields: true,
                fields: vec![crate::plugins::snippets::SnippetFieldDefinition::new(
                    "literal",
                )],
            },
        ]);
        data_cache.set_snapshot_for_test(snapshot);
        let snapshot = data_cache.snapshot();
        let commands = Vec::new();
        let actions_by_id = HashMap::new();
        let resolution = resolution_context(&snapshot, &commands, &actions_by_id);

        let opaque = HistoryPin {
            action_id: "clipboard:same body {{literal}}".into(),
            label: "Old saved snippet label".into(),
            desc: "Snippet".into(),
            args: Some("preserved args".into()),
            query: "old query".into(),
            timestamp: 1,
        };
        let displayed = CommandHistoryWidget::entry_from_pin(&resolution, &opaque);
        assert!(!displayed.missing);
        assert_eq!(displayed.action.label, "Old saved snippet label");
        assert_eq!(displayed.action.desc, "Snippet");
        assert_eq!(displayed.action.action, opaque.action_id);
        assert_eq!(displayed.action.args, opaque.args);

        let current_run = pin(&crate::plugins::snippets::snippet_run_action("second"));
        let displayed = CommandHistoryWidget::entry_from_pin(&resolution, &current_run);
        assert!(!displayed.missing);
        assert_eq!(displayed.action.label, "second");
        assert_eq!(displayed.action.action, current_run.action_id);

        let missing_run = pin(&crate::plugins::snippets::snippet_run_action("removed"));
        let displayed = CommandHistoryWidget::entry_from_pin(&resolution, &missing_run);
        assert!(displayed.missing);
        assert_eq!(displayed.action.action, missing_run.action_id);

        let history = HistoryEntry {
            query: "old query".into(),
            query_lc: "old query".into(),
            action: Action {
                label: "Opaque history literal".into(),
                desc: "Snippet".into(),
                action: "clipboard:same body {{literal}}".into(),
                args: None,
            },
            source: None,
            timestamp: 1,
        };
        let displayed = CommandHistoryWidget::entry_from_history(&resolution, &history);
        assert_eq!(displayed.action, history.action);
        assert!(!displayed.missing);
    }

    #[test]
    fn history_resolution_enumerates_one_current_catalog_per_prepare() {
        let workspace = workloads::IsolatedWorkspace::new();
        let mut plugins = PluginManager::new_inert_for_test();
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));
        let first_actions = Arc::new(RwLock::new(vec![action(
            "Zulu registration-first command",
            "plugin:shared",
            Some("arg"),
        )]));
        let second_actions = Arc::new(RwLock::new(vec![action(
            "Alpha registration-second command",
            "plugin:shared",
            Some("arg"),
        )]));
        plugins.register(Box::new(CountingCatalogPlugin {
            name: "catalog_first",
            actions: Arc::clone(&first_actions),
            calls: Arc::clone(&first_calls),
        }));
        plugins.register(Box::new(CountingCatalogPlugin {
            name: "catalog_second",
            actions: Arc::clone(&second_actions),
            calls: Arc::clone(&second_calls),
        }));

        let entries = (0..24)
            .map(|index| {
                history_entry(
                    action("Saved plugin label", "plugin:shared", Some("arg")),
                    index,
                )
            })
            .collect::<VecDeque<_>>();
        let _history_guard = crate::history::replace_history_for_test(entries);
        let data_cache = DashboardDataCache::new();
        let widget = CommandHistoryWidget::new(CommandHistoryConfig {
            count: 50,
            show_pinned_only: false,
            show_filter: true,
        });
        let actions = Vec::new();
        let actions_by_id = HashMap::new();
        let usage = HashMap::new();
        let both_enabled = HashSet::from(["catalog_first".to_owned(), "catalog_second".to_owned()]);

        let ctx = context_with_enabled(
            &data_cache,
            &plugins,
            &actions,
            &actions_by_id,
            &usage,
            Some(&both_enabled),
        );
        let first = widget.prepare_entries(&ctx);
        assert_eq!(first.len(), 24);
        assert!(
            first
                .iter()
                .all(|entry| entry.action.label == "Zulu registration-first command")
        );
        assert_eq!(first_calls.load(Ordering::Relaxed), 1);
        assert_eq!(second_calls.load(Ordering::Relaxed), 1);
        drop(ctx);

        *first_actions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = vec![action(
            "Zulu command after catalog rebuild",
            "plugin:shared",
            Some("arg"),
        )];
        let first_only = HashSet::from(["catalog_first".to_owned()]);
        let ctx = context_with_enabled(
            &data_cache,
            &plugins,
            &actions,
            &actions_by_id,
            &usage,
            Some(&first_only),
        );
        let rebuilt = widget.prepare_entries(&ctx);
        assert!(
            rebuilt
                .iter()
                .all(|entry| entry.action.label == "Zulu command after catalog rebuild")
        );
        assert_eq!(first_calls.load(Ordering::Relaxed), 2);
        assert_eq!(second_calls.load(Ordering::Relaxed), 1);
        drop(ctx);

        let second_only = HashSet::from(["catalog_second".to_owned()]);
        let ctx = context_with_enabled(
            &data_cache,
            &plugins,
            &actions,
            &actions_by_id,
            &usage,
            Some(&second_only),
        );
        let enabled = widget.prepare_entries(&ctx);
        assert!(
            enabled
                .iter()
                .all(|entry| entry.action.label == "Alpha registration-second command")
        );
        assert_eq!(first_calls.load(Ordering::Relaxed), 2);
        assert_eq!(second_calls.load(Ordering::Relaxed), 2);
        drop(ctx);

        *second_actions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Vec::new();
        let ctx = context_with_enabled(
            &data_cache,
            &plugins,
            &actions,
            &actions_by_id,
            &usage,
            Some(&second_only),
        );
        let removed = widget.prepare_entries(&ctx);
        assert!(
            removed
                .iter()
                .all(|entry| { entry.action.label == "Saved plugin label" && !entry.missing })
        );
        assert_eq!(second_calls.load(Ordering::Relaxed), 3);
        drop(ctx);
        drop(plugins);
        drop(workspace);
    }

    #[test]
    fn history_resolution_preserves_duplicate_order_and_exact_args() {
        let snapshot = DashboardDataSnapshot::default();
        let commands = vec![
            action("Zulu first, no args", "plugin:duplicate", None),
            action("Empty args, first", "plugin:duplicate", Some("")),
            action("Alpha second, no args", "plugin:duplicate", None),
            action("Empty args, second", "plugin:duplicate", Some("")),
        ];
        let actions_by_id = HashMap::new();
        let resolution = resolution_context(&snapshot, &commands, &actions_by_id);
        let saved = action("Saved label", "plugin:duplicate", None);

        assert_eq!(
            CommandHistoryWidget::resolve_action(&resolution, "plugin:duplicate", None, &saved,)
                .expect("first duplicate should resolve")
                .label,
            "Zulu first, no args"
        );
        assert_eq!(
            CommandHistoryWidget::resolve_action(
                &resolution,
                "plugin:duplicate",
                Some(""),
                &saved,
            )
            .expect("empty args should match only empty args")
            .label,
            "Empty args, first"
        );
        assert!(
            CommandHistoryWidget::resolve_action(
                &resolution,
                "plugin:duplicate",
                Some("different"),
                &saved,
            )
            .is_none()
        );
    }

    #[test]
    fn history_resolution_preserves_snippet_short_circuit_and_id_only_action_lookup() {
        let mut snapshot = DashboardDataSnapshot::default();
        snapshot.snippets = Arc::new(vec![crate::plugins::snippets::SnippetEntry {
            alias: "prompted".into(),
            text: "Hello {{name}}".into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![crate::plugins::snippets::SnippetFieldDefinition::new(
                "name",
            )],
        }]);
        let known_snippet = crate::plugins::snippets::snippet_run_action("prompted");
        let missing_snippet = crate::plugins::snippets::snippet_run_action("removed");
        let commands = vec![action("Plugin fallback", &missing_snippet, None)];
        let actions_by_id = HashMap::from([
            (
                known_snippet.clone(),
                action("Actions map collision", &known_snippet, None),
            ),
            (
                missing_snippet.clone(),
                action("Must not fall through", &missing_snippet, None),
            ),
            (
                "actions:current".into(),
                action("Current action by ID", "actions:current", None),
            ),
        ]);
        let resolution = resolution_context(&snapshot, &commands, &actions_by_id);

        let resolved = CommandHistoryWidget::resolve_action(
            &resolution,
            &known_snippet,
            None,
            &action("Saved snippet label", &known_snippet, None),
        )
        .expect("prompted snippet still resolves to its canonical action");
        assert_eq!(resolved.label, "prompted");
        assert_eq!(resolved.action, known_snippet);
        assert_eq!(resolved.desc, "Snippet");
        assert!(resolved.args.is_none());

        assert!(
            CommandHistoryWidget::resolve_action(
                &resolution,
                &known_snippet,
                Some("name=filled"),
                &action("Saved snippet label", &known_snippet, Some("name=filled")),
            )
            .is_none()
        );
        assert!(
            CommandHistoryWidget::resolve_action(
                &resolution,
                &missing_snippet,
                None,
                &action("Saved missing snippet", &missing_snippet, None),
            )
            .is_none()
        );

        let current = CommandHistoryWidget::resolve_action(
            &resolution,
            "actions:current",
            Some("old saved args"),
            &action(
                "Saved old action",
                "actions:current",
                Some("old saved args"),
            ),
        )
        .expect("actions_by_id keeps its ID-only matching behavior");
        assert_eq!(current.label, "Current action by ID");
        assert!(current.args.is_none());
    }

    #[test]
    fn history_resolution_uses_snapshot_precedence_and_rebuilds_fallback_actions() {
        let mut snapshot = DashboardDataSnapshot::default();
        snapshot.processes = Arc::new(vec![
            action("Process beats favorite", "shared:process", Some("x")),
            action("Process fallback", "process:only", Some("exact")),
            action("Process loses to plugin", "plugin:collision", Some("x")),
        ]);
        snapshot.favorites = Arc::new(vec![
            crate::plugins::fav::FavEntry {
                label: "Favorite loses to process".into(),
                action: "shared:process".into(),
                args: Some("x".into()),
            },
            crate::plugins::fav::FavEntry {
                label: "Favorite beats note fallback".into(),
                action: "note:open:note-favorite".into(),
                args: None,
            },
        ]);
        snapshot.notes = Arc::new(vec![
            crate::plugins::note::Note {
                title: "Note title".into(),
                path: Default::default(),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "note-only".into(),
                alias: Some("Primary note alias".into()),
                aliases: vec!["Primary note alias".into()],
                entity_refs: Vec::new(),
            },
            crate::plugins::note::Note {
                title: "Favorite note title".into(),
                path: Default::default(),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "note-favorite".into(),
                alias: Some("Favorite note alias".into()),
                aliases: vec!["Favorite note alias".into()],
                entity_refs: Vec::new(),
            },
        ]);
        snapshot.clipboard_history = Arc::new(vec!["Copied clipboard text".into()]);
        snapshot.todos = Arc::new(vec![crate::plugins::todo::TodoEntry {
            id: "todo-one".into(),
            text: "Ship the release".into(),
            done: true,
            priority: 1,
            tags: Vec::new(),
            entity_refs: Vec::new(),
        }]);
        snapshot.snippets = Arc::new(vec![crate::plugins::snippets::SnippetEntry {
            alias: "existing".into(),
            text: "snippet text".into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
        }]);

        let commands = vec![action("Plugin wins", "plugin:collision", Some("x"))];
        let actions_by_id = HashMap::new();
        let resolution = resolution_context(&snapshot, &commands, &actions_by_id);
        let resolve = |action_id: &str, args: Option<&str>| {
            CommandHistoryWidget::resolve_action(
                &resolution,
                action_id,
                args,
                &action("Saved label", action_id, args),
            )
        };

        assert_eq!(
            resolve("plugin:collision", Some("x"))
                .expect("plugin catalog wins over process snapshot")
                .label,
            "Plugin wins"
        );
        assert_eq!(
            resolve("shared:process", Some("x"))
                .expect("process snapshot wins over matching favorite")
                .label,
            "Process beats favorite"
        );
        assert!(resolve("process:only", None).is_none());
        assert_eq!(
            resolve("process:only", Some("exact"))
                .expect("process action args match exactly")
                .label,
            "Process fallback"
        );
        let favorite = resolve("note:open:note-favorite", None)
            .expect("favorite matching the note prefix wins first");
        assert_eq!(favorite.label, "Favorite beats note fallback");
        assert_eq!(favorite.desc, "Fav");

        let note = resolve("note:open:note-only", None).expect("current note resolves by slug");
        assert_eq!(note.label, "Primary note alias");
        assert_eq!(note.desc, "Note");
        assert!(note.args.is_none());

        let clipboard = resolve("clipboard:copy:0", None).expect("indexed clipboard still exists");
        assert_eq!(clipboard.label, "Copied clipboard text");
        assert_eq!(clipboard.desc, "Clipboard");
        assert!(resolve("clipboard:copy:1", None).is_none());

        assert_eq!(
            resolve("todo:done:0", None)
                .expect("done todo resolves")
                .label,
            "[x] Ship the release"
        );
        assert_eq!(
            resolve("todo:edit:0", None)
                .expect("todo edit resolves")
                .label,
            "[x] Ship the release"
        );
        assert_eq!(
            resolve("todo:remove:0", None)
                .expect("todo removal resolves")
                .label,
            "Remove todo Ship the release"
        );
        assert_eq!(
            resolve("snippet:edit:existing", Some("ignored"))
                .expect("snippet edit remains available")
                .label,
            "Edit snippet existing"
        );
        assert_eq!(
            resolve("snippet:remove:existing", None)
                .expect("snippet removal remains available")
                .label,
            "Remove snippet existing"
        );
    }

    #[test]
    fn history_resolution_snapshot_capture_survives_later_publication() {
        let cache = DashboardDataCache::new();
        let mut initial = DashboardDataSnapshot::default();
        initial.processes = Arc::new(vec![action(
            "Process snapshot before rename",
            "process:current",
            None,
        )]);
        cache.set_snapshot_for_test(initial);
        let captured = cache.snapshot();
        let commands = Vec::new();
        let actions_by_id = HashMap::new();
        let captured_context = resolution_context(&captured, &commands, &actions_by_id);

        let mut renamed = DashboardDataSnapshot::default();
        renamed.processes = Arc::new(vec![action(
            "Process snapshot after rename",
            "process:current",
            None,
        )]);
        cache.set_snapshot_for_test(renamed);

        assert_eq!(
            CommandHistoryWidget::resolve_action(
                &captured_context,
                "process:current",
                None,
                &action("Saved label", "process:current", None),
            )
            .expect("captured snapshot remains internally consistent")
            .label,
            "Process snapshot before rename"
        );
        let current = cache.snapshot();
        let current_context = resolution_context(&current, &commands, &actions_by_id);
        assert_eq!(
            CommandHistoryWidget::resolve_action(
                &current_context,
                "process:current",
                None,
                &action("Saved label", "process:current", None),
            )
            .expect("next preparation observes the new snapshot")
            .label,
            "Process snapshot after rename"
        );

        let mut deleted = DashboardDataSnapshot::default();
        deleted.processes = Arc::new(Vec::new());
        cache.set_snapshot_for_test(deleted);
        let latest = cache.snapshot();
        let latest_context = resolution_context(&latest, &commands, &actions_by_id);
        assert!(
            CommandHistoryWidget::resolve_action(
                &latest_context,
                "process:current",
                None,
                &action("Saved label", "process:current", None),
            )
            .is_none()
        );
        assert_eq!(
            CommandHistoryWidget::resolve_action(
                &current_context,
                "process:current",
                None,
                &action("Saved label", "process:current", None),
            )
            .expect("previous captured context still sees its snapshot")
            .label,
            "Process snapshot after rename"
        );
    }
}
