use crate::actions::Action;

use super::parse::{ActionKind, parse_action_kind};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LaunchPlan<'a> {
    pub action: ActionKind<'a>,
    pub args: Option<&'a str>,
}

pub(crate) fn plan_action<'a>(action: &'a Action) -> LaunchPlan<'a> {
    let parsed = parse_action_kind(action);
    let args = action.args.as_deref().or_else(|| {
        action
            .action
            .strip_prefix("clipboard_modify:execute:")
            .or_else(|| action.action.strip_prefix("clipboard_modify:undo:"))
    });
    LaunchPlan {
        args,
        action: parsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_volume_toggle_mute_without_side_effects() {
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: "volume:toggle_mute".into(),
            args: None,
        };
        assert_eq!(plan_action(&action).action, ActionKind::VolumeToggleMute);
    }

    #[test]
    fn plans_exec_path_fallback_with_args() {
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: "notepad.exe".into(),
            args: Some("foo.txt".into()),
        };
        assert_eq!(
            plan_action(&action).action,
            ActionKind::ExecPath {
                path: "notepad.exe",
                args: Some("foo.txt")
            }
        );
    }
}
