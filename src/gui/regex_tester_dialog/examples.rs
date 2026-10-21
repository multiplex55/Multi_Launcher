//! Browse bundled learning examples without changing the editable document.

use super::JumpTargets;
use crate::regex_tester::{BUILT_IN_EXAMPLES, RegexExample};
use eframe::egui;

#[derive(Default)]
pub(super) struct ExamplesState {
    pub query: String,
}

impl ExamplesState {
    pub fn entries(&self) -> impl Iterator<Item = &'static RegexExample> {
        let query = self.query.trim().to_lowercase();
        BUILT_IN_EXAMPLES.iter().filter(move |entry| {
            query.is_empty()
                || [entry.name, entry.description, entry.pattern]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&query))
        })
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<&'static RegexExample> {
        self.show_scoped(ui, &mut super::JumpTargets::new(None), true)
    }

    pub fn show_scoped(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut JumpTargets<'_>,
        committing: bool,
    ) -> Option<&'static RegexExample> {
        let mut load = None;
        let mut query = self.query.clone();
        egui::ScrollArea::vertical()
            .id_salt("regex_examples_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                let query_response = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id_salt("regex_examples_search")
                        .hint_text("Search examples")
                        .desired_width(ui.available_width()),
                );
                targets.text_entry(ui, &query_response, "examples-search", "Search examples");
                if committing && query_response.changed() {
                    self.query.clone_from(&query);
                }
                let mut found = false;
                for example in BUILT_IN_EXAMPLES.iter().filter(|entry| {
                    let query = query.trim().to_lowercase();
                    query.is_empty()
                        || [entry.name, entry.description, entry.pattern]
                            .iter()
                            .any(|field| field.to_lowercase().contains(&query))
                }) {
                    found = true;
                    ui.push_id(example.id, |ui| {
                        ui.strong(example.name);
                        // Keep the catalog's limitations visible rather than
                        // presenting extractors as comprehensive validators.
                        ui.label(example.description);
                        ui.label(egui::RichText::new(example.pattern).monospace());
                        ui.weak(format!(
                            "Rust flags: {} · all matches",
                            example.flags.suffix()
                        ));
                        ui.label("Sample text:");
                        ui.label(
                            egui::RichText::new(super::utf8_prefix(example.sample_text, 200))
                                .monospace(),
                        );
                        if example.sample_text.len() > 200 {
                            ui.weak("Sample preview; Load example uses the complete text.");
                        }
                        if let Some(replacement) = example.replacement {
                            ui.label("Replacement (stored for substitution testing):");
                            ui.label(
                                egui::RichText::new(super::utf8_prefix(replacement, 120))
                                    .monospace(),
                            );
                            if replacement.len() > 120 {
                                ui.weak(
                                    "Replacement preview; Load example uses the complete value.",
                                );
                            }
                        }
                        let load_response = ui.button("Load example");
                        targets.button(
                            ui,
                            &load_response,
                            format!("example-load/{}", example.id),
                            "Load example",
                        );
                        if load_response.clicked() && committing {
                            load = Some(example);
                        }
                        ui.separator();
                    });
                }
                if !found {
                    ui.weak("No examples match this search.");
                }
            });
        load
    }
}
