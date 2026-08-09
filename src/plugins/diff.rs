use crate::actions::Action;
use crate::diff::query::{
    DiffCommand, DiffOpenPayload, OPEN_PREFIX, encode_payload, parse_diff_query,
};
use crate::diff::settings::DiffConfigV1;
use crate::plugin::Plugin;
use eframe::egui;

#[derive(Debug, Clone, Default)]
pub struct DiffPlugin {
    settings: DiffConfigV1,
}
impl Plugin for DiffPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        match parse_diff_query(query) {
            None => vec![],
            Some(DiffCommand::Open) => vec![open_action(None, None)],
            Some(DiffCommand::OpenWithLeft { visible, .. }) => {
                vec![open_action(Some(visible), None)]
            }
            Some(DiffCommand::Compare {
                left_visible,
                right_visible,
                ..
            }) => vec![open_action(Some(left_visible), Some(right_visible))],
            Some(DiffCommand::Error(e)) => vec![Action {
                label: "Invalid diff command".into(),
                desc: e,
                action: "error".into(),
                args: None,
            }],
        }
    }
    fn name(&self) -> &str {
        "diff"
    }
    fn description(&self) -> &str {
        "Compare two files or folders"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "diff".into(),
            desc: "Open file and folder comparison".into(),
            action: format!(
                "{OPEN_PREFIX}{}",
                encode_payload(&DiffOpenPayload {
                    left: None,
                    right: None
                })
                .unwrap()
            ),
            args: None,
        }]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["diff"]
    }
    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(DiffConfigV1::default()).ok()
    }
    fn apply_settings(&mut self, value: &serde_json::Value) {
        self.settings = serde_json::from_value(value.clone()).unwrap_or_default();
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut c: DiffConfigV1 = serde_json::from_value(value.clone()).unwrap_or_default();
        ui.heading("Diff");
        ui.checkbox(&mut c.wrap_text, "Wrap text");
        ui.checkbox(&mut c.syntax_highlighting, "Syntax highlighting");
        ui.checkbox(&mut c.ignore_whitespace, "Ignore whitespace by default");
        ui.checkbox(&mut c.case_sensitive, "Case sensitive");
        ui.add(egui::Slider::new(&mut c.pane_split, 0.1..=0.9).text("Pane split"));
        self.settings = c.clone();
        *value = serde_json::to_value(c).unwrap();
    }
}
fn open_action(left: Option<String>, right: Option<String>) -> Action {
    let payload = encode_payload(&DiffOpenPayload {
        left: left.clone(),
        right: right.clone(),
    })
    .unwrap();
    Action {
        label: match (&left, &right) {
            (Some(l), Some(r)) => format!("Compare {l} ↔ {r}"),
            (Some(l), None) => format!("Open Diff with {l}"),
            _ => "Open Diff".into(),
        },
        desc: "Open native file and folder comparison".into(),
        action: format!("{OPEN_PREFIX}{payload}"),
        args: None,
    }
}
