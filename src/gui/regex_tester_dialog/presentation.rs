use crate::regex_tester::{EvaluationLimit, EvaluationSuspension};
use eframe::egui;

pub(super) fn compiler_label(message: &str, color: egui::Color32) -> egui::Label {
    egui::Label::new(egui::RichText::new(message).monospace().color(color)).wrap(true)
}

pub(super) fn suspension(reason: &EvaluationSuspension) -> String {
    let (name, unit) = match reason.limit {
        EvaluationLimit::PatternBytes => ("pattern size", "bytes"),
        EvaluationLimit::TextBytes => ("input size", "bytes"),
        EvaluationLimit::CaptureGroups => ("capture group count", "groups"),
        EvaluationLimit::StoredMatches => ("stored match count", "matches"),
        EvaluationLimit::MaterializedBytes => ("result size", "bytes"),
        EvaluationLimit::ReplacementBytes => ("replacement size", "bytes"),
        EvaluationLimit::ReplacementOutputBytes => ("replacement output size", "bytes"),
    };
    format!(
        "{name} exceeds {} {unit} (at least {} {unit}).",
        reason.maximum, reason.observed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_feedback_uses_readable_names_units_and_honest_lower_bounds() {
        for (limit, expected) in [
            (
                EvaluationLimit::TextBytes,
                "input size exceeds 65536 bytes (at least 65537 bytes).",
            ),
            (
                EvaluationLimit::ReplacementOutputBytes,
                "replacement output size exceeds 65536 bytes (at least 65537 bytes).",
            ),
            (
                EvaluationLimit::CaptureGroups,
                "capture group count exceeds 65536 groups (at least 65537 groups).",
            ),
        ] {
            assert_eq!(
                suspension(&EvaluationSuspension {
                    limit,
                    maximum: 65536,
                    observed: 65537
                }),
                expected
            );
        }
    }
}
