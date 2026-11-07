//! Bounded presentation of the session's complete local replacement result.

use super::JumpTargets;
use crate::regex_tester::{RegexSession, SubstitutionEvaluationResult};
use eframe::egui;
use std::time::Instant;

pub(super) const RESULT_PREVIEW_BYTES: usize = 16 * 1024;

pub(super) fn preview(output: &str) -> &str {
    super::utf8_prefix(output, RESULT_PREVIEW_BYTES)
}

/// Returns true for actual replacement/mode changes, never for result rendering.
pub(super) fn show(ui: &mut egui::Ui, session: &mut RegexSession) -> bool {
    show_scoped(ui, session, &mut JumpTargets::new(None), true)
}

#[derive(Default)]
pub(super) struct DeferredSubstitutionEffects {
    enabled: Option<bool>,
    replacement_changed: bool,
}
impl DeferredSubstitutionEffects {
    pub(super) fn apply(self, session: &mut RegexSession) {
        if let Some(enabled) = self.enabled {
            session.set_substitution_enabled(enabled, Instant::now());
        }
        if self.replacement_changed {
            session.mark_replacement_changed(Instant::now());
        }
    }
}

pub(super) fn show_scoped(
    ui: &mut egui::Ui,
    session: &mut RegexSession,
    targets: &mut JumpTargets<'_>,
    committing: bool,
) -> bool {
    show_scoped_inner(ui, session, targets, committing, None)
}

pub(super) fn show_scoped_deferred(
    ui: &mut egui::Ui,
    session: &mut RegexSession,
    targets: &mut JumpTargets<'_>,
    committing: bool,
    deferred: &mut DeferredSubstitutionEffects,
) -> bool {
    show_scoped_inner(ui, session, targets, committing, Some(deferred))
}

fn show_scoped_inner(
    ui: &mut egui::Ui,
    session: &mut RegexSession,
    targets: &mut JumpTargets<'_>,
    committing: bool,
    mut deferred: Option<&mut DeferredSubstitutionEffects>,
) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_salt("regex_substitution_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height().max(1.0))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            let mut enabled = session.substitution_enabled();
            let response = ui.checkbox(&mut enabled, "Enable substitution preview");
            targets.checkbox(
                ui,
                &response,
                "substitution/enabled",
                "Enable substitution preview",
            );
            if response.changed() && committing {
                if let Some(effects) = deferred.as_mut() {
                    effects.enabled = Some(enabled);
                } else {
                    session.set_substitution_enabled(enabled, Instant::now());
                }
                changed = true;
            }
            ui.label("Replacement");
            ui.weak("Rust replacement syntax: $1, ${name}, and $$ for a literal dollar.");
            if session.draft.replacement.len()
                > crate::regex_tester::policy::NORMAL_REPLACEMENT_BYTES
                && session.draft.replacement.len() <= session.policy().replacement_bytes
            {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Large replacement: above the normal 1 KiB range.",
                );
            }
            let maximum_bytes = session.policy().replacement_bytes;
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                super::editor_layout::plain(
                    ui,
                    text.as_str(),
                    width,
                    maximum_bytes,
                    egui::TextStyle::Monospace.resolve(ui.style()),
                    false,
                )
            };
            if session.draft.replacement.len() > session.policy().replacement_bytes {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "Replacement retained ({} bytes); exceeds the {} byte interactive limit.",
                        session.draft.replacement.len(),
                        session.policy().replacement_bytes
                    ),
                );
                ui.label(
                    egui::RichText::new(super::utf8_prefix(&session.draft.replacement, 128))
                        .monospace(),
                );
                let clear = ui.button("Clear replacement");
                targets.button(ui, &clear, "substitution/clear", "Clear replacement");
                if clear.clicked() && committing {
                    session.draft.replacement.clear();
                    if let Some(effects) = deferred.as_mut() {
                        effects.replacement_changed = true;
                    } else {
                        session.mark_replacement_changed(Instant::now());
                    }
                    changed = true;
                }
            } else {
                let mut shadow = (!committing).then(|| session.draft.replacement.clone());
                let replacement = shadow.as_mut().unwrap_or(&mut session.draft.replacement);
                let response = ui.add(
                    egui::TextEdit::multiline(replacement)
                        .id(egui::Id::new("regex_substitution_replacement"))
                        .font(egui::TextStyle::Monospace)
                        .layouter(&mut layouter)
                        .desired_width(ui.available_width())
                        .desired_rows(3),
                );
                targets.text_entry(
                    ui,
                    &response,
                    "substitution/replacement",
                    "Replacement text",
                );
                if response.changed() && committing {
                    if let Some(effects) = deferred.as_mut() {
                        effects.replacement_changed = true;
                    } else {
                        session.mark_replacement_changed(Instant::now());
                    }
                    changed = true;
                }
            }
            ui.separator();
            ui.strong("Replacement result");
            if !session.substitution_enabled() {
                ui.weak(
                    "Enable substitution to preview changes. Input remains editable and unchanged.",
                );
                return;
            }
            match session.substitution_result() {
                Some(SubstitutionEvaluationResult::Success(result)) => {
                    let text = preview(&result.output);
                    ui.weak(format!(
                        "{} replacements · {} output bytes",
                        result.replacements_made,
                        result.output.len()
                    ));
                    if text.len() < result.output.len() {
                        ui.weak(format!(
                            "Showing the first {} bytes; the complete result is retained.",
                            text.len()
                        ));
                    }
                    let mut borrowed = text;
                    egui::ScrollArea::both()
                        .id_salt("regex_substitution_result_scroll")
                        .max_height(200.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut borrowed)
                                    .id_salt("regex_substitution_result")
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(ui.available_width())
                                    .desired_rows(5),
                            );
                        });
                }
                Some(SubstitutionEvaluationResult::InvalidPattern(error)) => {
                    ui.add(super::presentation::compiler_label(
                        &error.message,
                        ui.visuals().error_fg_color,
                    ));
                }
                Some(SubstitutionEvaluationResult::Suspended(reason)) => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "Replacement preview paused: {}",
                            super::presentation::suspension(reason)
                        ),
                    );
                }
                None => {
                    ui.weak("Replacement preview pending…");
                }
            }
        });
    changed
}

#[cfg(test)]
mod deferred_tests {
    #[test]
    fn substitution_mode_changes_only_when_applied() {
        let mut session = crate::regex_tester::RegexSession::default();
        let effects = super::DeferredSubstitutionEffects {
            enabled: Some(true),
            replacement_changed: false,
        };
        assert!(!session.substitution_enabled());
        effects.apply(&mut session);
        assert!(session.substitution_enabled());
    }
}
