use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(cmd) = query.strip_prefix("sh ") {
            if !cmd.trim().is_empty() {
                return vec![Action {
                    label: format!("Run `{}`", cmd),
                    desc: "Shell".into(),
                    action: format!("shell:{}", cmd),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run arbitrary shell commands (prefix: `sh`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}

