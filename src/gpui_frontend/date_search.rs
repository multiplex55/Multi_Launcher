//! Provenance-bound Date Arithmetic Search result admission.
use multi_launcher::actions::Action;
use multi_launcher::commands::{ClipboardCommand, Command, parse_action};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DateRoute {
    Copy(String),
    Diagnostic(String),
}

pub(super) fn classify(name: &str, action: &Action) -> Option<DateRoute> {
    if name != "date_arithmetic"
        || action.args.is_some()
        || action.desc != "Date Arithmetic"
    {
        return None;
    }
    if let Some(message) = action.action.strip_prefix("noop:") {
        return (!message.is_empty() && action.label == message)
            .then(|| DateRoute::Diagnostic(message.to_owned()));
    }
    match parse_action(action).ok()? {
        Command::Clipboard(ClipboardCommand::SetText { text })
            if action.action == format!("clipboard:{text}") =>
        {
            Some(DateRoute::Copy(text))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{DateRoute, classify};
    use multi_launcher::{plugin::Plugin, plugins::date_arithmetic::DateArithmeticPlugin};

    #[test]
    fn published_date_result_keeps_exact_payload_and_origin() {
        let query = "date days between 2026-10-05 and 2026-12-25";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let action = DateArithmeticPlugin
            .search_with_reference(query, now)
            .remove(0);
        assert_eq!(action.action, "clipboard:81 days");
        assert_eq!(
            classify("date_arithmetic", &action),
            Some(DateRoute::Copy("81 days".into()))
        );
        assert!(classify("other", &action).is_none());
        let mut altered = action.clone();
        altered.args = Some(String::new());
        assert!(classify("date_arithmetic", &altered).is_none());
        let bad = DateArithmeticPlugin.search("date invalid input").remove(0);
        assert!(matches!(
            classify("date_arithmetic", &bad),
            Some(DateRoute::Diagnostic(_))
        ));
    }
}
