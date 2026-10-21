//! Explicit clipboard workflow presentation and readable match formatting.

use super::JumpTargets;
use crate::clipboard_modify::clipboard::ClipboardError;
use crate::regex_tester::{
    CaptureValue, EvaluationResult, MatchCompleteness, RegexMatch, RegexSession,
    SubstitutionEvaluationResult,
};
use eframe::egui;
use std::fmt::Write;

pub(super) enum ClipboardAction {
    ImportText,
    CopyPattern,
    CopyMatch,
    CopyCapture(usize),
    CopyAllMatches,
    CopyReplacementResult,
    CopyMatchInformation,
}

pub(super) fn error_message(error: &ClipboardError) -> String {
    match error {
        ClipboardError::NonText => "Clipboard does not contain text.".into(),
        ClipboardError::Busy(message) => format!("Clipboard is busy: {message}"),
        ClipboardError::Transient(message) => {
            format!("Clipboard is temporarily unavailable: {message}")
        }
        ClipboardError::InvalidContent(message) => {
            format!("Clipboard text could not be accessed: {message}")
        }
        ClipboardError::Permanent(message) => format!("Clipboard is unavailable: {message}"),
        _ => error.to_string(),
    }
}

pub(super) fn all_matches(matches: &[RegexMatch]) -> String {
    let mut text = String::new();
    for (index, matched) in matches.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(&matched.text);
    }
    text
}

pub(super) fn all_matches_label(session: &RegexSession) -> &'static str {
    if matches!(
        session.result(),
        Some(EvaluationResult::Success {
            completeness: MatchCompleteness::Truncated { .. },
            ..
        })
    ) {
        "Copy displayed matches"
    } else {
        "Copy All Matches"
    }
}

pub(super) fn match_information(matched: &RegexMatch) -> String {
    let mut text = format!(
        "Match #{}\nLine {}, column {} (Unicode scalars)\n{}\nText{}:\n{}\n",
        matched.id.index() + 1,
        matched.location.line,
        matched.location.column,
        super::inspection::byte_label(matched.span),
        if matched.text.is_empty() {
            " (zero-width match, empty)"
        } else {
            ""
        },
        matched.text,
    );
    for capture in &matched.captures {
        let _ = writeln!(text, "{}", super::inspection::capture_label(capture));
        match &capture.value {
            CaptureValue::Unmatched => {
                text.push_str("Unmatched optional capture\n");
            }
            CaptureValue::Matched { span, text: value } => {
                let _ = writeln!(text, "{}", super::inspection::byte_label(*span));
                if value.is_empty() {
                    text.push_str("Matched empty capture\n");
                } else {
                    let _ = writeln!(text, "Matched text:\n{value}");
                }
            }
        }
    }
    text
}

pub(super) fn show(
    ui: &mut egui::Ui,
    session: &RegexSession,
    selected_capture: usize,
) -> Option<ClipboardAction> {
    show_scoped(ui, session, selected_capture, &mut JumpTargets::new(None))
}

pub(super) fn show_scoped(
    ui: &mut egui::Ui,
    session: &RegexSession,
    selected_capture: usize,
    targets: &mut JumpTargets<'_>,
) -> Option<ClipboardAction> {
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("regex_clipboard_controls_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height().max(1.0))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.weak("Clipboard access happens only when you press an action.");
            let import = ui.button("Use Clipboard Text");
            targets.button(ui, &import, "clipboard/import", "Import clipboard text");
            if import.clicked() {
                action = Some(ClipboardAction::ImportText);
            }
            ui.weak("Replaces only input / test text; keeps pattern, flags and replacement.");
            let copy_pattern = ui.button("Copy Pattern");
            targets.button(ui, &copy_pattern, "clipboard/copy-pattern", "Copy pattern");
            if copy_pattern.clicked() {
                action = Some(ClipboardAction::CopyPattern);
            }
            let matched = session
                .selected_index()
                .and_then(|index| session.matches().get(index));
            let copy_match =
                ui.add_enabled(matched.is_some(), egui::Button::new("Copy Selected Match"));
            targets.button(
                ui,
                &copy_match,
                "clipboard/copy-match",
                "Copy selected match",
            );
            if copy_match.clicked() {
                action = Some(ClipboardAction::CopyMatch);
            }
            let capture_available = matched
                .and_then(|matched| {
                    super::inspection::copy_text(
                        matched,
                        super::inspection::CopyTarget::Capture(selected_capture),
                    )
                })
                .is_some();
            let copy_capture = ui.add_enabled(
                capture_available,
                egui::Button::new("Copy Selected Capture"),
            );
            targets.button(
                ui,
                &copy_capture,
                "clipboard/copy-capture",
                "Copy selected capture",
            );
            if copy_capture.clicked() {
                action = Some(ClipboardAction::CopyCapture(selected_capture));
            }
            ui.weak("Choose the capture in Selected match.");
            let truncated = matches!(
                session.result(),
                Some(EvaluationResult::Success {
                    completeness: MatchCompleteness::Truncated { .. },
                    ..
                })
            );
            let all_label = all_matches_label(session);
            let copy_all =
                ui.add_enabled(!session.matches().is_empty(), egui::Button::new(all_label));
            targets.button(ui, &copy_all, "clipboard/copy-matches", "Copy matches");
            if copy_all.clicked() {
                action = Some(ClipboardAction::CopyAllMatches);
            }
            ui.weak(format!(
                "{} {} values, joined with newlines; zero-width matches keep empty values.",
                session.matches().len(),
                if truncated { "displayed" } else { "current" }
            ));
            let copy_information = ui.add_enabled(
                matched.is_some(),
                egui::Button::new("Copy Match Information"),
            );
            targets.button(
                ui,
                &copy_information,
                "clipboard/copy-match-information",
                "Copy match information",
            );
            if copy_information.clicked() {
                action = Some(ClipboardAction::CopyMatchInformation);
            }
            let result_available = session.substitution_enabled()
                && matches!(
                    session.substitution_result(),
                    Some(SubstitutionEvaluationResult::Success(_))
                );
            let copy_result = ui.add_enabled(
                result_available,
                egui::Button::new("Copy Replacement Result"),
            );
            targets.button(
                ui,
                &copy_result,
                "clipboard/copy-result",
                "Copy replacement result",
            );
            if copy_result.clicked() {
                action = Some(ClipboardAction::CopyReplacementResult);
            }
            ui.weak("Copies the complete current output, including content beyond the preview.");
        });
    action
}
