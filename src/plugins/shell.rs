use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

pub const SHELL_CMDS_FILE: &str = "shell_cmds.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct ShellCmdEntry {
    pub name: String,
    pub args: String,
}

pub fn load_shell_cmds(path: &str) -> anyhow::Result<Vec<ShellCmdEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<ShellCmdEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_shell_cmds(path: &str, cmds: &[ShellCmdEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(cmds)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("sh") {
            return vec![Action {
                label: "sh: edit saved commands".into(),
                desc: "Shell".into(),
                action: "shell:dialog".into(),
                args: None,
            }];
        }

        if let Some(cmd) = trimmed.strip_prefix("sh ") {
            let arg = cmd.trim();
            if arg.is_empty() {
                return Vec::new();
            }
            if let Ok(list) = load_shell_cmds(SHELL_CMDS_FILE) {
                let matcher = SkimMatcherV2::default();
                let mut best: Option<(ShellCmdEntry, i64)> = None;
                for entry in list {
                    if let Some(score) = matcher.fuzzy_match(&entry.name, arg) {
                        if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
                            best = Some((entry, score));
                        }
                    }
                }
                if let Some((entry, _)) = best {
                    return vec![Action {
                        label: format!("Run {}", entry.name),
                        desc: "Shell".into(),
                        action: format!("shell:{}", entry.args),
                        args: None,
                    }];
                }
            }
            return vec![Action {
                label: format!("Run `{}`", arg),
                desc: "Shell".into(),
                action: format!("shell:{}", arg),
                args: None,
            }];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run arbitrary shell commands (prefix: `sh`; type `sh` to edit presets)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}

