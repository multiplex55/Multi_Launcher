use crate::actions::Action;
use crate::common::command::{parse_args, ParseArgsResult};
use crate::plugin::Plugin;

pub struct BrightnessPlugin;

const BRIGHT_USAGE: &str = "Usage: bright <0-100>";

fn usage_action(usage: &str) -> Action {
    Action {
        label: usage.into(),
        desc: "Brightness".into(),
        action: "query:bright ".into(),
        args: None,
    }
}

impl Plugin for BrightnessPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let lowered = trimmed.to_ascii_lowercase();
        if lowered.len() >= 2 && "bright".starts_with(&lowered) && lowered != "bright" {
            return vec![usage_action(BRIGHT_USAGE)];
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "bright") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "bright: edit brightness".into(),
                    desc: "Brightness".into(),
                    action: "brightness:dialog".into(),
                    args: None,
                }, usage_action(BRIGHT_USAGE)];
            }
            let rest = rest.trim();
            let args: Vec<&str> = rest.split_whitespace().collect();
            match parse_args(&args, BRIGHT_USAGE, |args| {
                if args.len() == 1 {
                    args[0].parse::<u8>().ok().filter(|val| *val <= 100)
                } else {
                    None
                }
            }) {
                ParseArgsResult::Parsed(val) => {
                    return vec![Action {
                        label: format!("Set brightness to {val}%"),
                        desc: "Brightness".into(),
                        action: format!("brightness:set:{val}"),
                        args: None,
                    }];
                }
                ParseArgsResult::Usage(usage) => {
                    return vec![usage_action(&usage)];
                }
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str { "brightness" }

    fn description(&self) -> &str {
        "Adjust display brightness (prefix: `bright`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "bright".into(),
            desc: "Brightness".into(),
            action: "query:bright ".into(),
            args: None,
        }]
    }
}
