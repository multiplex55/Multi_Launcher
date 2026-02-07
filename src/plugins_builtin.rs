use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::calc_history::{self, CalcHistoryEntry, CALC_HISTORY_FILE, MAX_ENTRIES};
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
                preview_text: None,
                risk_level: None,
                icon: None,
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
            preview_text: None,
            risk_level: None,
            icon: None,
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
                    preview_text: None,
                    risk_level: None,
                    icon: None,
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
                            preview_text: None,
                            risk_level: None,
                            icon: None,
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
                            preview_text: None,
                            risk_level: None,
                            icon: None,
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
        "Evaluate mathematical expressions (prefix: `=`; `= history` to list)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "=".into(),
                desc: "Calculator".into(),
                action: "query:= ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "= history".into(),
                desc: "Calculator".into(),
                action: "query:= history".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "calc list".into(),
                desc: "Calculator".into(),
                action: "query:calc list".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
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
