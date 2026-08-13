use crate::{actions::Action, mkmacro::MkMacroStore, plugin::Plugin};
use eframe::egui;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MkMacroPluginSettings {
    pub enabled: bool,
}
impl Default for MkMacroPluginSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub struct MkMacroPlugin {
    store: Arc<MkMacroStore>,
    matcher: SkimMatcherV2,
    settings: MkMacroPluginSettings,
}
impl MkMacroPlugin {
    pub fn new(store: Arc<MkMacroStore>) -> Self {
        Self {
            store,
            matcher: SkimMatcherV2::default(),
            settings: Default::default(),
        }
    }
    fn results(&self, filter: &str, enabled_only: bool) -> Vec<Action> {
        if !self.settings.enabled {
            return vec![];
        }
        let mut matches: Vec<_> = self
            .store
            .snapshot()
            .macros
            .iter()
            .filter(|m| {
                (!enabled_only || m.enabled)
                    && (filter.is_empty()
                        || self.matcher.fuzzy_match(&m.name, filter).is_some()
                        || self.matcher.fuzzy_match(&m.description, filter).is_some())
            })
            .cloned()
            .collect();
        matches.sort_by_key(|m| {
            if filter.is_empty() {
                0
            } else {
                -self
                    .matcher
                    .fuzzy_match(&format!("{} {}", m.name, m.description), filter)
                    .unwrap_or(i64::MIN)
            }
        });
        matches
            .into_iter()
            .map(|m| Action {
                label: if m.enabled {
                    m.name
                } else {
                    format!("{} (disabled)", m.name)
                },
                desc: if m.description.is_empty() {
                    "Mouse/keyboard macro".into()
                } else {
                    m.description
                },
                action: if m.enabled {
                    format!("mkmacro:run:{}", m.id)
                } else {
                    "mkmacro:dialog".into()
                },
                args: None,
            })
            .collect()
    }
}
impl Plugin for MkMacroPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !self.settings.enabled {
            return vec![];
        }
        let q = query.trim();
        if q.eq_ignore_ascii_case("mkmacro") {
            return vec![Action {
                label: "Open Mouse/Keyboard Macros".into(),
                desc: "Mouse and keyboard automation macros".into(),
                action: "mkmacro:dialog".into(),
                args: None,
            }];
        }
        let Some(rest) = crate::common::strip_prefix_ci(q, "mkmacro ") else {
            return vec![];
        };
        if rest.trim().eq_ignore_ascii_case("list") {
            self.results("", true)
        } else {
            self.results(rest.trim(), false)
        }
    }
    fn name(&self) -> &str {
        "mkmacro"
    }
    fn description(&self) -> &str {
        "Mouse and keyboard automation macros"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["mkmacro"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "mkmacro".into(),
                desc: self.description().into(),
                action: "query:mkmacro".into(),
                args: None,
            },
            Action {
                label: "mkmacro list".into(),
                desc: self.description().into(),
                action: "query:mkmacro list".into(),
                args: None,
            },
        ]
    }
    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(MkMacroPluginSettings::default()).ok()
    }
    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(v) = serde_json::from_value(value.clone()) {
            self.settings = v;
        }
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut s: MkMacroPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        if ui
            .checkbox(&mut s.enabled, "Enable mouse/keyboard macros")
            .changed()
        {
            *value = serde_json::to_value(s).unwrap();
        }
    }
}
