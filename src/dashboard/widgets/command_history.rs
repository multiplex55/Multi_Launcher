use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::history::{toggle_pin, HistoryEntry, HistoryPin, HISTORY_PINS_FILE};
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
    last_pins_load: Instant,
}

impl CommandHistoryWidget {
    pub fn new(cfg: CommandHistoryConfig) -> Self {
        Self {
            cfg,
            filter: String::new(),
            cached_pins: Vec::new(),
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

    fn refresh_pins(&mut self) {
        if self.last_pins_load.elapsed() > Duration::from_secs(2) {
            self.cached_pins = crate::history::load_pins(HISTORY_PINS_FILE).unwrap_or_default();
            self.last_pins_load = Instant::now();
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
        preview_text: None,
        risk_level: None,
        icon: None,
    ) -> Option<Action> {
        if let Some(action) = ctx.actions_by_id.get(action_id) {
            return Some(action.clone());
        }

        let commands = ctx.plugins.commands_filtered(ctx.enabled_plugins);
        if let Some(action) = commands
            .into_iter()
            .find(|action| action.action == action_id && action.args.as_deref() == args)
        {
            return Some(action);
        }

        let snapshot = ctx.data_cache.snapshot();
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
                preview_text: None,
                risk_level: None,
                icon: None,
            });
        }

        if let Some(slug) = action_id.strip_prefix("note:open:") {
            if let Some(note) = snapshot.notes.iter().find(|note| note.slug == slug) {
                return Some(Action {
                    label: note.alias.as_ref().unwrap_or(&note.title).clone(),
                    desc: "Note".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        if let Some(idx) = action_id
            .strip_prefix("clipboard:copy:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(entry) = snapshot.clipboard_history.get(idx) {
                return Some(Action {
                    label: entry.clone(),
                    desc: "Clipboard".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        if let Some(idx) = action_id
            .strip_prefix("todo:done:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(todo) = snapshot.todos.get(idx) {
                return Some(Action {
                    label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                    desc: "Todo".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        if let Some(idx) = action_id
            .strip_prefix("todo:edit:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(todo) = snapshot.todos.get(idx) {
                return Some(Action {
                    label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                    desc: "Todo".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        if let Some(idx) = action_id
            .strip_prefix("todo:remove:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(todo) = snapshot.todos.get(idx) {
                return Some(Action {
                    label: format!("Remove todo {}", todo.text),
                    desc: "Todo".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        for snippet in snapshot.snippets.iter() {
            if action_id == format!("clipboard:{}", snippet.text) {
                return Some(Action {
                    label: snippet.alias.clone(),
                    desc: "Snippet".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        if let Some(alias) = action_id.strip_prefix("snippet:edit:") {
            if snapshot.snippets.iter().any(|s| s.alias == alias) {
                return Some(Action {
                    label: format!("Edit snippet {alias}"),
                    desc: "Snippet".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        if let Some(alias) = action_id.strip_prefix("snippet:remove:") {
            if snapshot.snippets.iter().any(|s| s.alias == alias) {
                return Some(Action {
                    label: format!("Remove snippet {alias}"),
                    desc: "Snippet".into(),
                    action: action_id.to_string(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }

        None
    }

    fn entry_from_history(ctx: &DashboardContext<'_>, entry: &HistoryEntry) -> DisplayEntry {
        let resolved =
            Self::resolve_action(ctx, &entry.action.action, entry.action.args.as_deref());
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
            preview_text: None,
            risk_level: None,
            icon: None,
        };
        let resolved = Self::resolve_action(ctx, &pin.action_id, pin.args.as_deref());
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
        self.refresh_pins();
        let mut clicked = None;
        ui.label("Command history");

        if self.cfg.show_filter {
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.filter);
            });
        }

        let history_entries =
            crate::history::with_history(|h| h.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();

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
                        preview_text: None,
                        risk_level: None,
                        icon: None,
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
                    let _ = crate::history::remove_pin(
                        HISTORY_PINS_FILE,
                        &entry.action_id,
                        entry.action.args.as_deref(),
                    );
                    self.cached_pins
                        .retain(|p| p.action_id != entry.action_id || p.args != entry.action.args);
                }
                ui.label(timestamp);
            });
        }

        clicked
    }
}
