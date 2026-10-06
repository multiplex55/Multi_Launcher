use chrono::{NaiveDate, NaiveDateTime};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::date_arithmetic::DateArithmeticPlugin;
use multi_launcher::plugins::timestamp::TimestampPlugin;

fn fixed_reference() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 5)
        .unwrap()
        .and_hms_nano_opt(14, 30, 12, 345_600_000)
        .unwrap()
}

#[test]
fn calendar_arithmetic_uses_domain_presentation_and_clipboard() {
    let actions =
        DateArithmeticPlugin.search_with_reference("date 2026-10-05 + 30 days", fixed_reference());
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].label, "Wednesday, November 4, 2026 — 2026-11-04");
    assert_eq!(actions[0].desc, "Date Arithmetic");
    assert_eq!(actions[0].action, "clipboard:2026-11-04");
}

#[test]
fn relative_arithmetic_uses_the_explicit_reference() {
    let actions =
        DateArithmeticPlugin.search_with_reference("date 3 hours from now", fixed_reference());
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].label,
        "Monday, October 5, 2026 17:30:12.3456 — 2026-10-05 17:30:12.3456"
    );
    assert_eq!(actions[0].action, "clipboard:2026-10-05 17:30:12.3456");
}

#[test]
fn difference_copies_the_signed_value_and_unit() {
    let actions = DateArithmeticPlugin.search_with_reference(
        "date days between 2026-10-05 and Christmas",
        fixed_reference(),
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].label, "81 days");
    assert_eq!(actions[0].action, "clipboard:81 days");
}

#[test]
fn date_time_results_preserve_seconds_and_fractional_seconds() {
    let actions = DateArithmeticPlugin
        .search_with_reference("date 2026-10-05T14:30:12.345600", fixed_reference());
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].label,
        "Monday, October 5, 2026 14:30:12.3456 — 2026-10-05 14:30:12.3456"
    );
    assert_eq!(actions[0].action, "clipboard:2026-10-05 14:30:12.3456");
}

#[test]
fn recognized_invalid_date_requests_return_nonexecuting_helpful_actions() {
    for query in ["date 2026-02-30", "date today + 1 fortnight"] {
        let actions = DateArithmeticPlugin.search_with_reference(query, fixed_reference());
        assert_eq!(actions.len(), 1, "{query}");
        assert!(actions[0].action.starts_with("noop:"), "{query}");
        assert_eq!(actions[0].desc, "Date Arithmetic");
        assert!(!actions[0].label.is_empty());
    }
}

#[test]
fn bare_date_is_empty_and_commands_remain_discoverable() {
    assert!(
        DateArithmeticPlugin
            .search_with_reference("date", fixed_reference())
            .is_empty()
    );
    assert!(
        DateArithmeticPlugin
            .search_with_reference("date   ", fixed_reference())
            .is_empty()
    );
    assert!(DateArithmeticPlugin.commands().iter().any(|command| {
        command.label == "date <expression>" && command.action == "query:date "
    }));
}

#[test]
fn date_prefix_is_case_insensitive_whole_token_and_unicode_safe() {
    let plugin = DateArithmeticPlugin;
    for query in [
        "DATE 2026-10-05",
        "date\t2026-10-05",
        "date\u{00a0}2026-10-05",
        "\u{00a0}date\u{00a0}2026-10-05",
    ] {
        assert_eq!(
            plugin.search_with_reference(query, fixed_reference())[0].action,
            "clipboard:2026-10-05",
            "{query:?}"
        );
    }
    for query in [
        "datefoo 2026-10-05",
        "dateé 2026-10-05",
        "🗓️date 2026-10-05",
        "🌍",
    ] {
        assert!(
            plugin
                .search_with_reference(query, fixed_reference())
                .is_empty(),
            "{query:?}"
        );
    }
}

#[test]
fn date_plugin_does_not_claim_timestamp_prefixes() {
    let date = DateArithmeticPlugin;
    assert!(
        date.search_with_reference("ts 0", fixed_reference())
            .is_empty()
    );
    assert!(
        date.search_with_reference("tsm 3600000", fixed_reference())
            .is_empty()
    );

    assert_eq!(TimestampPlugin.search("ts 0").len(), 1);
    assert_eq!(TimestampPlugin.search("tsm 3600000").len(), 1);
}

#[test]
fn ordinary_plugin_search_uses_the_local_reference_at_the_boundary() {
    let actions = DateArithmeticPlugin.search("date now");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].desc, "Date Arithmetic");
    assert!(actions[0].label.contains(" — "));
}
