use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult, edit_typed_settings,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QuickToolEntry {
    pub query: String,
    #[serde(default)]
    pub auto_submit: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum QuickToolEntryRepr {
    Legacy(String),
    Current {
        query: String,
        #[serde(default)]
        auto_submit: bool,
    },
}

impl<'de> Deserialize<'de> for QuickToolEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match QuickToolEntryRepr::deserialize(deserializer)? {
            QuickToolEntryRepr::Legacy(query) => Self::manual(query),
            QuickToolEntryRepr::Current { query, auto_submit } => Self { query, auto_submit },
        })
    }
}

impl QuickToolEntry {
    fn manual(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            auto_submit: false,
        }
    }
}

fn default_queries() -> Vec<QuickToolEntry> {
    ["sys", "net", "info cpu", "vol", "bright"]
        .into_iter()
        .map(QuickToolEntry::manual)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuickToolsConfig {
    #[serde(default = "default_queries")]
    pub queries: Vec<QuickToolEntry>,
}

impl Default for QuickToolsConfig {
    fn default() -> Self {
        Self {
            queries: default_queries(),
        }
    }
}

pub struct QuickToolsWidget {
    cfg: QuickToolsConfig,
}

impl QuickToolsWidget {
    pub fn new(cfg: QuickToolsConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut QuickToolsConfig, _ctx| {
            let mut changed = false;
            let mut remove_idx = None;
            for (idx, entry) in cfg.queries.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Tool {}", idx + 1));
                    changed |= ui.text_edit_singleline(&mut entry.query).changed();
                    changed |= ui.checkbox(&mut entry.auto_submit, "Auto submit").changed();
                    if ui.small_button("✕").clicked() {
                        remove_idx = Some(idx);
                    }
                });
            }
            if let Some(idx) = remove_idx {
                cfg.queries.remove(idx);
                changed = true;
            }
            if ui.button("Add tool").clicked() {
                cfg.queries.push(QuickToolEntry::manual(""));
                changed = true;
            }
            changed
        })
    }

    fn action_for(entry: &QuickToolEntry) -> Option<WidgetAction> {
        let query = entry.query.trim();
        if query.is_empty() {
            return None;
        }
        Some(WidgetAction {
            action: Action {
                label: query.to_string(),
                desc: "Tool".into(),
                action: format!(
                    "{}:{query}",
                    if entry.auto_submit {
                        "queryexec"
                    } else {
                        "query"
                    }
                ),
                args: None,
            },
            query_override: (!entry.auto_submit).then(|| query.to_string()),
        })
    }
}

impl Default for QuickToolsWidget {
    fn default() -> Self {
        Self::new(QuickToolsConfig::default())
    }
}

impl Widget for QuickToolsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        if self.cfg.queries.is_empty() {
            ui.label("Add tools in the widget settings.");
            return None;
        }

        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for entry in &self.cfg.queries {
                let Some(action) = Self::action_for(entry) else {
                    continue;
                };
                let label = if entry.auto_submit {
                    format!("{} ↵", action.action.label)
                } else {
                    action.action.label.clone()
                };
                let response = ui.button(label);
                let response = if entry.auto_submit {
                    response.on_hover_text("Auto submit enabled")
                } else {
                    response
                };
                if response.clicked() {
                    clicked = Some(action);
                }
            }
        });
        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<QuickToolsConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_legacy_structured_mixed_and_missing_flag_entries() {
        let legacy: QuickToolsConfig =
            serde_json::from_value(json!({"queries":["fs","mm","cm"]})).unwrap();
        assert_eq!(
            legacy
                .queries
                .iter()
                .map(|e| e.query.as_str())
                .collect::<Vec<_>>(),
            ["fs", "mm", "cm"]
        );
        assert!(legacy.queries.iter().all(|entry| !entry.auto_submit));

        let current: QuickToolsConfig = serde_json::from_value(json!({"queries":[
            {"query":"sys","auto_submit":true}, {"query":"net","auto_submit":false}
        ]}))
        .unwrap();
        assert!(current.queries[0].auto_submit);
        assert!(!current.queries[1].auto_submit);

        let mixed: QuickToolsConfig = serde_json::from_value(json!({"queries":[
            "legacy", {"query":"current","auto_submit":true}, {"query":"missing"}, ""
        ]}))
        .unwrap();
        assert_eq!(mixed.queries.len(), 4);
        assert!(!mixed.queries[0].auto_submit);
        assert!(mixed.queries[1].auto_submit);
        assert!(!mixed.queries[2].auto_submit);
        assert_eq!(mixed.queries[3].query, "");
        assert!(QuickToolsWidget::action_for(&mixed.queries[3]).is_none());
    }

    #[test]
    fn serialization_is_always_canonical_after_legacy_input() {
        let cfg: QuickToolsConfig = serde_json::from_value(json!({"queries":["fs"]})).unwrap();
        assert_eq!(
            serde_json::to_value(cfg).unwrap(),
            json!({"queries":[{"query":"fs","auto_submit":false}]})
        );
    }

    #[test]
    fn defaults_and_new_entries_are_manual() {
        assert!(default_queries().iter().all(|entry| !entry.auto_submit));
        assert_eq!(
            QuickToolEntry::manual(""),
            QuickToolEntry {
                query: String::new(),
                auto_submit: false
            }
        );
    }

    #[test]
    fn entry_edits_remain_independent_and_middle_removal_preserves_neighbors() {
        let mut entries = vec![
            QuickToolEntry {
                query: "one".into(),
                auto_submit: true,
            },
            QuickToolEntry::manual("two"),
            QuickToolEntry {
                query: "three".into(),
                auto_submit: true,
            },
        ];
        entries[1].auto_submit = true;
        assert!(entries[0].auto_submit);
        entries.remove(1);
        assert_eq!(
            entries,
            vec![
                QuickToolEntry {
                    query: "one".into(),
                    auto_submit: true
                },
                QuickToolEntry {
                    query: "three".into(),
                    auto_submit: true
                },
            ]
        );
    }

    #[test]
    fn actions_obey_manual_auto_submit_and_whitespace_contracts() {
        let manual =
            QuickToolsWidget::action_for(&QuickToolEntry::manual("  Fs  internal  Space "))
                .unwrap();
        assert_eq!(manual.action.label, "Fs  internal  Space");
        assert_eq!(manual.action.action, "query:Fs  internal  Space");
        assert_eq!(
            manual.query_override.as_deref(),
            Some("Fs  internal  Space")
        );

        let automatic = QuickToolsWidget::action_for(&QuickToolEntry {
            query: "  MM x ".into(),
            auto_submit: true,
        })
        .unwrap();
        assert_eq!(automatic.action.label, "MM x");
        assert_eq!(automatic.action.action, "queryexec:MM x");
        assert_eq!(automatic.query_override, None);
        assert!(QuickToolsWidget::action_for(&QuickToolEntry::manual(" \t ")).is_none());
    }
}
