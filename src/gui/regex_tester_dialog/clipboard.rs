//! Explicit clipboard workflow presentation and readable match formatting.

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
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_source("regex_clipboard_controls_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height().max(1.0))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap = Some(true);
            ui.weak("Clipboard access happens only when you press an action.");
            if ui.button("Use Clipboard Text").clicked() {
                action = Some(ClipboardAction::ImportText);
            }
            ui.weak("Replaces only input / test text; keeps pattern, flags and replacement.");
            if ui.button("Copy Pattern").clicked() {
                action = Some(ClipboardAction::CopyPattern);
            }
            let matched = session
                .selected_index()
                .and_then(|index| session.matches().get(index));
            if ui
                .add_enabled(matched.is_some(), egui::Button::new("Copy Selected Match"))
                .clicked()
            {
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
            if ui
                .add_enabled(
                    capture_available,
                    egui::Button::new("Copy Selected Capture"),
                )
                .clicked()
            {
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
            if ui
                .add_enabled(!session.matches().is_empty(), egui::Button::new(all_label))
                .clicked()
            {
                action = Some(ClipboardAction::CopyAllMatches);
            }
            ui.weak(format!(
                "{} {} values, joined with newlines; zero-width matches keep empty values.",
                session.matches().len(),
                if truncated { "displayed" } else { "current" }
            ));
            if ui
                .add_enabled(
                    matched.is_some(),
                    egui::Button::new("Copy Match Information"),
                )
                .clicked()
            {
                action = Some(ClipboardAction::CopyMatchInformation);
            }
            let result_available = session.substitution_enabled()
                && matches!(
                    session.substitution_result(),
                    Some(SubstitutionEvaluationResult::Success(_))
                );
            if ui
                .add_enabled(
                    result_available,
                    egui::Button::new("Copy Replacement Result"),
                )
                .clicked()
            {
                action = Some(ClipboardAction::CopyReplacementResult);
            }
            ui.weak("Copies the complete current output, including content beyond the preview.");
        });
    action
}
