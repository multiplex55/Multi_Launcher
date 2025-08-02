use crate::actions::Action;
use crate::history::with_history;
use crate::plugin::Plugin;
use eframe::egui;

const MAX_HISTORY_RESULTS: usize = 10;

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
        const PREFIX: &str = "hi";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        if let Some(clear_rest) = crate::common::strip_prefix_ci(query.trim(), "hi clear") {
            if clear_rest.is_empty() {
                return vec![Action {
                    label: "Clear history".into(),
                    desc: "History".into(),
                    action: "history:clear".into(),
                    args: None,
                }];
            }
        }
        let filter = rest.trim().to_lowercase();
        with_history(|h| {
            h.iter()
                .enumerate()
                .filter(|(_, entry)| entry.query_lc.contains(&filter))
                .take(MAX_HISTORY_RESULTS)
                .map(|(idx, entry)| Action {
                    label: entry.query.clone(),
                    desc: "History".into(),
                    action: format!("history:{idx}"),
                    args: None,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn name(&self) -> &str {
        "history"
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
            ui.add(egui::Slider::new(&mut cfg.max_entries, 10..=500).text(""));
        });
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize history settings: {e}"),
        }
    }
}
