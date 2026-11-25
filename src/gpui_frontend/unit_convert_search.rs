//! Provenance-bound inline Unit Convert results.
use multi_launcher::actions::Action;
use multi_launcher::commands::{ClipboardCommand, Command, parse_action};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum UnitRoute {
    Copy(String),
    Diagnostic(String),
}

pub(super) fn classify(name: &str, action: &Action) -> Option<UnitRoute> {
    if name != "unit_convert" || action.args.is_some() {
        return None;
    }
    if let Some(message) = action.action.strip_prefix("noop:") {
        return (action.desc == "Unit convert" && !message.is_empty() && action.label == message)
            .then(|| UnitRoute::Diagnostic(message.to_owned()));
    }
    if !matches!(
        action.desc.as_str(),
        "Unit convert" | "Approximate unit conversion"
    ) {
        return None;
    }
    match parse_action(action).ok()? {
        Command::Clipboard(ClipboardCommand::SetText { text })
            if action.action.strip_prefix("clipboard:") == Some(text.as_str()) =>
        {
            Some(UnitRoute::Copy(text))
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
    let plugin = multi_launcher::plugins::unit_convert::UnitConvertPlugin;
    super::search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin],
        query,
        enabled,
        caps,
    )
    .iter()
    .any(|(name, row)| name == "unit_convert" && row == action)
}

#[cfg(test)]
mod tests {
    use super::{UnitRoute, classify};
    use multi_launcher::{plugin::Plugin, plugins::unit_convert::UnitConvertPlugin};

    #[test]
    fn unit_rows_preserve_case_approximation_and_diagnostics() {
        let plugin = UnitConvertPlugin;
        let exact = plugin.search("CONVERT 1 MB to Mb").remove(0);
        assert_eq!(
            classify("unit_convert", &exact),
            Some(UnitRoute::Copy("8 Mb".into()))
        );
        assert!(classify("base_convert", &exact).is_none());
        let approximate = plugin.search("conv 1 month to day").remove(0);
        assert_eq!(approximate.desc, "Approximate unit conversion");
        assert_eq!(
            classify("unit_convert", &approximate),
            Some(UnitRoute::Copy("30 day".into()))
        );
        let diagnostic = plugin.search("conv 3 km to kg").remove(0);
        assert!(matches!(
            classify("unit_convert", &diagnostic),
            Some(UnitRoute::Diagnostic(_))
        ));
        let mut altered = diagnostic.clone();
        altered.label.push('!');
        assert!(classify("unit_convert", &altered).is_none());
        altered = exact;
        altered.args = Some(String::new());
        assert!(classify("unit_convert", &altered).is_none());
    }
}
