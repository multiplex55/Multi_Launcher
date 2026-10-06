use crate::actions::Action;
use crate::date_arithmetic::{DateEvaluationOutcome, evaluate_expression};
use crate::plugin::Plugin;
use chrono::{Local, NaiveDateTime};

pub struct DateArithmeticPlugin;

impl DateArithmeticPlugin {
    /// Search with an explicit local reference for deterministic callers and tests.
    pub fn search_with_reference(&self, query: &str, reference_now: NaiveDateTime) -> Vec<Action> {
        let Some(expression) = date_expression(query) else {
            return Vec::new();
        };
        if expression.is_empty() {
            return Vec::new();
        }

        match evaluate_expression(expression, reference_now) {
            Ok(outcome) => vec![result_action(outcome)],
            Err(error) => {
                let message = error.to_string();
                vec![Action {
                    label: message.clone(),
                    desc: "Date Arithmetic".into(),
                    action: format!("noop:{message}"),
                    args: None,
                }]
            }
        }
    }
}

fn result_action(outcome: DateEvaluationOutcome) -> Action {
    Action {
        label: outcome.display_label,
        desc: "Date Arithmetic".into(),
        action: format!("clipboard:{}", outcome.clipboard_payload),
        args: None,
    }
}

fn date_expression(query: &str) -> Option<&str> {
    let query = query.trim_start();
    if !query
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("date"))
    {
        return None;
    }

    let rest = query.get(4..)?;
    if rest.is_empty() {
        return Some("");
    }
    rest.chars()
        .next()
        .filter(|character| character.is_whitespace())
        .map(|_| rest.trim_start())
}

impl Plugin for DateArithmeticPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        self.search_with_reference(query, Local::now().naive_local())
    }

    fn name(&self) -> &str {
        "date_arithmetic"
    }

    fn description(&self) -> &str {
        "Date arithmetic and date differences (prefix: `date`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["date"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "date <expression>".into(),
                desc: "Calculate a date or date-time".into(),
                action: "query:date ".into(),
                args: None,
            },
            Action {
                label: "date today + 7 days".into(),
                desc: "Add a calendar offset".into(),
                action: "query:date today + 7 days".into(),
                args: None,
            },
            Action {
                label: "date days between A and B".into(),
                desc: "Calculate a signed date difference".into(),
                action: "query:date days between ".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::date_expression;

    #[test]
    fn recognizes_whole_prefix_tokens_without_unicode_slicing() {
        assert_eq!(date_expression("DATE 2026-10-05"), Some("2026-10-05"));
        assert_eq!(date_expression("  date\t2026-10-05"), Some("2026-10-05"));
        assert_eq!(
            date_expression("date\u{00a0}2026-10-05"),
            Some("2026-10-05")
        );
        assert_eq!(date_expression("date"), Some(""));
        assert_eq!(date_expression("datefoo 2026-10-05"), None);
        assert_eq!(date_expression("dateé 2026-10-05"), None);
        assert_eq!(date_expression("🗓️date 2026-10-05"), None);
    }
}
