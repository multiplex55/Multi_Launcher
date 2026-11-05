use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::calc_history::{self, CALC_HISTORY_FILE, CalcHistoryEntry, MAX_ENTRIES};
use eframe::egui;
use serde::{Deserialize, Serialize};
use urlencoding::encode;

pub struct WebSearchPlugin;

impl Plugin for WebSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.starts_with("g ") && query.len() > 2 {
            let q = &query[2..];
            vec![Action {
                label: format!("Search Google for {q}"),
                desc: "Web search".into(),
                action: format!("https://www.google.com/search?q={}", encode(q)),
                args: None,
            }]
        } else {
            Vec::new()
        }
    }

    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Perform web searches using Google (prefix: `g`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "g".into(),
            desc: "Web search".into(),
            action: "query:g ".into(),
            args: None,
        }]
    }
}

#[derive(Default)]
pub struct CalculatorPlugin {
    save_on_enter: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct CalculatorPluginSettings {
    #[serde(default)]
    save_on_enter: bool,
}

impl Plugin for CalculatorPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("calc pad") {
            return vec![Action {
                label: "Open Calculation Pad".into(),
                desc: "Open the multi-line pad for exact integers and unit arithmetic".into(),
                action: "calc:pad".into(),
                args: None,
            }];
        }
        if trimmed.eq_ignore_ascii_case("calc list")
            || trimmed.eq_ignore_ascii_case("= history")
            || trimmed.eq_ignore_ascii_case("= list")
        {
            return calc_history::load_history(CALC_HISTORY_FILE)
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(idx, entry)| Action {
                    label: format!("{} = {}", entry.expr, entry.result),
                    desc: "Calculator".into(),
                    action: format!("calc:history:{idx}"),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = trimmed.strip_prefix('=') {
            let expr = rest.trim();
            if expr.is_empty() {
                return Vec::new();
            }
            match exmex::eval_str::<f64>(expr) {
                Ok(v) => {
                    let result = v.to_string();
                    if self.save_on_enter {
                        vec![Action {
                            label: format!("{} = {}", expr, result),
                            desc: "Calculator".into(),
                            action: format!("calc:{}", result),
                            args: Some(expr.to_string()),
                        }]
                    } else {
                        let entry = CalcHistoryEntry {
                            expr: expr.to_string(),
                            result: result.clone(),
                        };
                        let _ = calc_history::append_entry(CALC_HISTORY_FILE, entry, MAX_ENTRIES);
                        vec![Action {
                            label: format!("{} = {}", expr, result),
                            desc: "Calculator".into(),
                            action: format!("calc:{}", result),
                            args: None,
                        }]
                    }
                }
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        }
    }

    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Evaluate inline scalar expressions or open the multi-line pad for exact integers and unit arithmetic (prefix: `=`; `= history` to list)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "calc pad".into(),
                desc: "Open multi-line calculations with exact integers and unit arithmetic".into(),
                action: "query:calc pad".into(),
                args: None,
            },
            Action {
                label: "=".into(),
                desc: "Calculator".into(),
                action: "query:= ".into(),
                args: None,
            },
            Action {
                label: "= history".into(),
                desc: "Calculator".into(),
                action: "query:= history".into(),
                args: None,
            },
            Action {
                label: "calc list".into(),
                desc: "Calculator".into(),
                action: "query:calc list".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(CalculatorPluginSettings {
            save_on_enter: self.save_on_enter,
        })
        .ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(s) = serde_json::from_value::<CalculatorPluginSettings>(value.clone()) {
            self.save_on_enter = s.save_on_enter;
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: CalculatorPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.checkbox(&mut cfg.save_on_enter, "Save on Enter");
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize calculator settings: {e}"),
        }
        self.save_on_enter = cfg.save_on_enter;
    }
}

#[cfg(test)]
mod calculator_plugin_tests {
    use super::*;

    #[test]
    fn calc_pad_is_an_exact_opener_and_inline_calculator_behavior_is_preserved() {
        let plugin = CalculatorPlugin {
            save_on_enter: true,
        };
        let actions = plugin.search("  CaLc PaD  ");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "calc:pad");
        assert_eq!(actions[0].args, None);
        assert!(actions[0].desc.contains("exact integers"));
        assert!(plugin.search("calc pad more").is_empty());

        let command = plugin
            .commands()
            .into_iter()
            .find(|command| command.action == "query:calc pad")
            .expect("the original Calculation Pad command remains registered");
        assert_eq!(command.label, "calc pad");
        assert!(command.desc.contains("unit arithmetic"));
        assert!(plugin.description().contains("inline scalar expressions"));

        let inline = plugin.search("= 1 + 1");
        assert_eq!(inline.len(), 1);
        assert_eq!(inline[0].action, "calc:2");
        assert_eq!(inline[0].args.as_deref(), Some("1 + 1"));
    }
}
