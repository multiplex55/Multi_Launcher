use crate::actions::Action;
use crate::history::{HistoryEntry, with_history};
use crate::plugin::Plugin;
use eframe::egui;
use std::collections::VecDeque;

const MAX_HISTORY_RESULTS: usize = 10;

/// One stock History row with its absolute index into the same snapshot.
pub struct HistoryProjectionRow<'a> {
    pub index: usize,
    pub entry: &'a HistoryEntry,
    pub action: Action,
}

pub fn is_history_clear(query: &str) -> bool {
    crate::common::strip_prefix_ci(query.trim(), "hi clear").is_some_and(str::is_empty)
}

/// Exact stock clear row for a matching History query.
pub fn clear_action(query: &str) -> Option<Action> {
    is_history_clear(query).then(|| Action { label: "Clear history".into(), desc: "History".into(), action: "history:clear".into(), args: None })
}

/// Project stock newest-first results; the ten-row cap precedes frontend filtering.
pub fn project_history_rows<'a>(
    query: &str,
    history: &'a VecDeque<HistoryEntry>,
) -> Vec<HistoryProjectionRow<'a>> {
    let Some(rest) = crate::common::strip_prefix_ci(query, "hi") else {
        return Vec::new();
    };
    if is_history_clear(query) {
        return Vec::new();
    }
    let filter = rest.trim().to_lowercase();
    history.iter().enumerate()
        .filter(|(_, entry)| entry.query_lc.contains(&filter))
        .take(MAX_HISTORY_RESULTS)
        .map(|(index, entry)| HistoryProjectionRow {
            index,
            entry,
            action: Action {
                label: entry.query.clone(),
                desc: "History".into(),
                action: format!("history:{index}"),
                args: None,
            },
        })
        .collect()
}

pub struct HistoryPlugin;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct HistoryPluginSettings {
    pub max_entries: usize,
}

impl Default for HistoryPluginSettings {
    fn default() -> Self {
        Self { max_entries: 100 }
    }
}

impl Plugin for HistoryPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if crate::common::strip_prefix_ci(query, "hi").is_none() { return Vec::new(); }
        if let Some(action) = clear_action(query) { return vec![action]; }
        with_history(|h| project_history_rows(query, h).into_iter().map(|row| row.action).collect())
        .unwrap_or_default()
    }

    fn name(&self) -> &str {
        <Self as crate::plugin::BuiltinIdentity>::ID
    }

    fn description(&self) -> &str {
        "Search previously executed queries (prefix: `hi`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "hi".into(),
                desc: "History".into(),
                action: "query:hi".into(),
                args: None,
            },
            Action {
                label: "hi clear".into(),
                desc: "History".into(),
                action: "query:hi clear".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(HistoryPluginSettings::default()).ok()
    }

    fn apply_settings(&mut self, _value: &serde_json::Value) {}

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: HistoryPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("History limit");
            let response = ui.add(egui::Slider::new(&mut cfg.max_entries, 10..=500).text(""));
            crate::settings_editor::render::observe_scoped_settings_navigation_response(
                ui,
                &response,
                "plugin.history.max_entries",
                crate::gui::jump_mode::TargetKind::Slider,
            );
        });
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize history settings: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_history_clear, project_history_rows};
    use crate::actions::Action;
    use crate::history::HistoryEntry;
    use std::collections::VecDeque;

    fn entry(query: &str) -> HistoryEntry {
        HistoryEntry {
            query: query.into(),
            query_lc: query.to_lowercase(),
            action: Action { label: query.into(), desc: "App".into(), action: "fixture.exe".into(), args: None },
            source: None,
            timestamp: 0,
        }
    }

    #[test]
    fn history_projection_uses_absolute_indices_and_stock_cap() {
        let history: VecDeque<_> = (0..15)
            .map(|index| entry(&format!("Café {index}")))
            .collect();
        let rows = project_history_rows("HI CAFÉ", &history);
        assert_eq!(rows.len(), 10);
        assert_eq!(
            rows.iter().map(|row| row.index).collect::<Vec<_>>(),
            (0..10).collect::<Vec<_>>()
        );
        assert_eq!(rows[0].entry.query, "Café 0");
        assert_eq!(rows[9].action.action, "history:9");
        let filtered = project_history_rows("hi 12", &history);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].index, 12);
        assert_eq!(filtered[0].action.action, "history:12");
        assert!(project_history_rows("other", &history).is_empty());
        assert!(is_history_clear("hi clear"));
        assert!(project_history_rows("hi clear", &history).is_empty());
        assert!(!is_history_clear("hi clear extra"));
    }
}
