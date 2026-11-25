//! Provenance-bound Base Convert clipboard result admission.
use multi_launcher::actions::Action;
use multi_launcher::commands::{ClipboardCommand, Command, parse_action};

pub(super) fn classify(name: &str, action: &Action) -> Option<String> {
    if name != "base_convert" || action.desc != "Base Convert" || action.args.is_some() {
        return None;
    }
    match parse_action(action).ok()? {
        Command::Clipboard(ClipboardCommand::SetText { text })
            if action.action.strip_prefix("clipboard:") == Some(text.as_str()) =>
        {
            #[cfg(windows)]
            if text.contains('\0') {
                return None;
            }
            Some(text)
        }
        _ => None,
    }
}

pub(super) fn is_current(
    query: &str,
    action: &Action,
    enabled: Option<&std::collections::HashSet<String>>,
    caps: Option<&std::collections::HashMap<String, Vec<String>>>,
) -> bool {
    let plugin = multi_launcher::plugins::base_convert::BaseConvertPlugin;
    super::search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin],
        query,
        enabled,
        caps,
    )
    .iter()
    .any(|(name, row)| name == "base_convert" && row == action)
}

#[cfg(test)]
mod tests {
    use super::classify;
    use multi_launcher::{
        actions::Action, plugin::Plugin, plugins::base_convert::BaseConvertPlugin,
    };

    #[test]
    fn base_convert_copies_exact_payload_but_refuses_reserved_commands() {
        let plugin = BaseConvertPlugin;
        for (query, expected) in [
            ("conv 255 dec to hex", "ff"),
            ("convert 4869c3a9 hex to text", "Hié"),
            ("CONVERT 20436166c3a920 hex to text", " Café "),
            ("conv 636f70793a6e6f7065 hex to text", "copy:nope"),
        ] {
            let action = plugin.search(query).remove(0);
            assert_eq!(classify("base_convert", &action).as_deref(), Some(expected));
            assert!(classify("other", &action).is_none());
            let mut changed = action.clone();
            changed.args = Some(String::new());
            assert!(classify("base_convert", &changed).is_none());
        }
        #[cfg(windows)]
        {
            let action = plugin.search("conv 610062 hex to text").remove(0);
            assert_eq!(action.action, "clipboard:a\0b");
            assert!(classify("base_convert", &action).is_none());
        }
        for payload in ["clear", "dialog", "copy:0", "copy:123"] {
            let action = Action {
                label: String::new(),
                desc: "Base Convert".into(),
                action: format!("clipboard:{payload}"),
                args: None,
            };
            assert!(classify("base_convert", &action).is_none());
        }
    }
}
