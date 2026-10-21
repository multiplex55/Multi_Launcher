//! Local catalog browsing; actions are explicit and handled by the dialog.

use super::JumpTargets;
use crate::regex_tester::{
    ReferenceCategory, ReferenceEntry, ReferenceSyntaxKind, search_reference,
};
use eframe::egui;

#[derive(Default)]
pub(super) struct ReferenceState {
    pub query: String,
    pub category: Option<ReferenceCategory>,
}

pub(super) enum ReferenceAction {
    Append(&'static ReferenceEntry),
    Copy(&'static ReferenceEntry),
}

impl ReferenceState {
    pub fn entries(&self) -> impl Iterator<Item = &'static ReferenceEntry> {
        search_reference(&self.query, self.category)
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<ReferenceAction> {
        self.show_scoped(ui, &mut super::JumpTargets::new(None), true)
    }

    pub fn show_scoped(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut JumpTargets<'_>,
        committing: bool,
    ) -> Option<ReferenceAction> {
        let mut action = None;
        let mut query = self.query.clone();
        let mut category = self.category;
        egui::ScrollArea::vertical()
            .id_salt("regex_reference_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                let query_response = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id_salt("regex_reference_search")
                        .hint_text("Search reference")
                        .desired_width(ui.available_width()),
                );
                targets.text_entry(ui, &query_response, "reference-search", "Search reference");
                let popup_scope = targets.combo_options_scope("reference-category");
                let mut popup_targets = JumpTargets::new(popup_scope.as_ref());
                let category_response = egui::ComboBox::from_id_salt("regex_reference_category")
                    .width(ui.available_width().min(180.0))
                    .selected_text(category.map_or("All categories", ReferenceCategory::title))
                    .show_ui(ui, |ui| {
                        let response = ui.selectable_value(&mut category, None, "All categories");
                        popup_targets.selectable(
                            ui,
                            &response,
                            "category/all",
                            "All reference categories",
                        );
                        for option in ReferenceCategory::ALL {
                            let response =
                                ui.selectable_value(&mut category, Some(option), option.title());
                            popup_targets.selectable(
                                ui,
                                &response,
                                format!("category/{}", category_index(option)),
                                "Reference category",
                            );
                        }
                    });
                targets.combo_box(
                    ui,
                    &category_response.response,
                    "reference-category",
                    "Reference category",
                );
                let mut found = false;
                for entry in search_reference(&query, category) {
                    found = true;
                    ui.push_id(entry.id, |ui| {
                        ui.strong(entry.title);
                        ui.label(egui::RichText::new(entry.syntax).monospace());
                        ui.weak(match entry.syntax_kind {
                            ReferenceSyntaxKind::Pattern => "Pattern: can compile by itself",
                            ReferenceSyntaxKind::Fragment => {
                                "Fragment: needs surrounding pattern context"
                            }
                        });
                        ui.label(entry.description);
                        if let Some(example) = entry.example {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Example: {}\nText: {}",
                                    example.pattern, example.text
                                ))
                                .monospace(),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            let append = ui.button("Append syntax");
                            targets.button(
                                ui,
                                &append,
                                format!("reference/{}/append", entry.id),
                                "Append reference syntax",
                            );
                            if append.clicked() && committing {
                                action = Some(ReferenceAction::Append(entry));
                            }
                            let copy = ui.button("Copy syntax");
                            targets.button(
                                ui,
                                &copy,
                                format!("reference/{}/copy", entry.id),
                                "Copy reference syntax",
                            );
                            if copy.clicked() && committing {
                                action = Some(ReferenceAction::Copy(entry));
                            }
                        });
                        ui.separator();
                    });
                }
                if !found {
                    ui.weak("No reference entries match these filters.");
                }
            });
        if committing {
            self.query = query;
            self.category = category;
        }
        action
    }
}

fn category_index(category: ReferenceCategory) -> usize {
    ReferenceCategory::ALL
        .iter()
        .position(|candidate| *candidate == category)
        .unwrap_or(0)
}
