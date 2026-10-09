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
        ctx: &DashboardContext<'_>,
        action_id: &str,
        args: Option<&str>,
        saved_action: &Action,
    ) -> Option<Action> {
        let _timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::HistoryResolve);
        let snapshot = ctx.data_cache.snapshot();
        if action_id.starts_with("snippet:run:") {
            return crate::plugins::snippets::resolve_snippet_run_action_from_entries(
                action_id,
                args,
                &snapshot.snippets,
            );
        }

        if let Some(action) = ctx.actions_by_id.get(action_id) {
            return Some(action.clone());
        }

        let commands = {
            let mut catalog_timer = crate::performance::MetricTimer::start(
                crate::performance::Metric::HistoryCatalogBuild,
            );
            catalog_timer.set_work_units(0);
            let commands = ctx.plugins.commands_filtered(ctx.enabled_plugins);
            catalog_timer.set_work_units(commands.len() as u64);
            commands
        };
        if let Some(action) = commands
            .into_iter()
            .find(|action| action.action == action_id && action.args.as_deref() == args)
        {
            return Some(action);
        }

        if let Some(action) = snapshot
            .processes
            .iter()
            .find(|action| action.action == action_id && action.args.as_deref() == args)
        {
            return Some(action.clone());
        }

        if let Some(fav) = snapshot
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
            && let Some(note) = snapshot.notes.iter().find(|note| note.slug == slug)
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
            && let Some(entry) = snapshot.clipboard_history.get(idx)
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
            && let Some(todo) = snapshot.todos.get(idx)
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
            && let Some(todo) = snapshot.todos.get(idx)
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
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("Remove todo {}", todo.text),
                desc: "Todo".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(alias) = action_id.strip_prefix("snippet:edit:")
            && snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Edit snippet {alias}"),
                desc: "Snippet".into(),
                action: action_id.to_string(),
                args: None,
            });
        }

        if let Some(alias) = action_id.strip_prefix("snippet:remove:")
            && snapshot.snippets.iter().any(|s| s.alias == alias)
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

    fn entry_from_history(ctx: &DashboardContext<'_>, entry: &HistoryEntry) -> DisplayEntry {
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

    fn entry_from_pin(ctx: &DashboardContext<'_>, pin: &HistoryPin) -> DisplayEntry {
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

        let mut prepare_timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::HistoryPrepare);
        prepare_timer.set_work_units(0);
        let history_entries =
            crate::history::with_history(|h| h.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
        prepare_timer.set_work_units(history_entries.len() as u64);

        let mut entries: Vec<DisplayEntry> = Vec::new();
        if self.cfg.show_pinned_only {
            entries.extend(
                self.cached_pins
                    .iter()
                    .map(|pin| Self::entry_from_pin(ctx, pin)),
            );
        } else {
            let mut pinned: Vec<DisplayEntry> = self
                .cached_pins
                .iter()
                .map(|pin| Self::entry_from_pin(ctx, pin))
                .collect();
            pinned.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
            entries.extend(pinned);

            for entry in &history_entries {
                if Self::is_pinned(&self.cached_pins, entry) {
                    continue;
                }
                entries.push(Self::entry_from_history(ctx, entry));
            }
        }

        let filtered = entries
            .into_iter()
            .filter(|entry| Self::entry_matches_filter(entry, &self.filter))
            .take(self.cfg.count)
            .collect::<Vec<_>>();
        drop(prepare_timer);

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
    use crate::plugin::PluginManager;
    use std::collections::HashMap;

    fn context<'a>(
        data_cache: &'a DashboardDataCache,
        plugins: &'a PluginManager,
        actions: &'a [Action],
        actions_by_id: &'a HashMap<String, Action>,
        usage: &'a HashMap<String, u32>,
    ) -> DashboardContext<'a> {
        DashboardContext {
            actions,
            actions_by_id,
            usage,
            plugins,
            enabled_plugins: None,
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
        let plugins = PluginManager::new();
        let actions = Vec::new();
        let actions_by_id = HashMap::new();
        let usage = HashMap::new();
        let ctx = context(&data_cache, &plugins, &actions, &actions_by_id, &usage);

        let opaque = HistoryPin {
            action_id: "clipboard:same body {{literal}}".into(),
            label: "Old saved snippet label".into(),
            desc: "Snippet".into(),
            args: Some("preserved args".into()),
            query: "old query".into(),
            timestamp: 1,
        };
        let displayed = CommandHistoryWidget::entry_from_pin(&ctx, &opaque);
        assert!(!displayed.missing);
        assert_eq!(displayed.action.label, "Old saved snippet label");
        assert_eq!(displayed.action.desc, "Snippet");
        assert_eq!(displayed.action.action, opaque.action_id);
        assert_eq!(displayed.action.args, opaque.args);

        let current_run = pin(&crate::plugins::snippets::snippet_run_action("second"));
        let displayed = CommandHistoryWidget::entry_from_pin(&ctx, &current_run);
        assert!(!displayed.missing);
        assert_eq!(displayed.action.label, "second");
        assert_eq!(displayed.action.action, current_run.action_id);

        let missing_run = pin(&crate::plugins::snippets::snippet_run_action("removed"));
        let displayed = CommandHistoryWidget::entry_from_pin(&ctx, &missing_run);
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
        let displayed = CommandHistoryWidget::entry_from_history(&ctx, &history);
        assert_eq!(displayed.action, history.action);
    }
}
