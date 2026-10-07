//! Local catalog browsing; actions are explicit and handled by the dialog.

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
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_source("regex_reference_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap = Some(true);
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id_source("regex_reference_search")
                        .hint_text("Search reference")
                        .desired_width(ui.available_width()),
                );
                egui::ComboBox::from_id_source("regex_reference_category")
                    .width(ui.available_width().min(180.0))
                    .selected_text(
                        self.category
                            .map_or("All categories", ReferenceCategory::title),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.category, None, "All categories");
                        for category in ReferenceCategory::ALL {
                            ui.selectable_value(
                                &mut self.category,
                                Some(category),
                                category.title(),
                            );
                        }
                    });
                let mut found = false;
                for entry in self.entries() {
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
                            if ui.button("Append syntax").clicked() {
                                action = Some(ReferenceAction::Append(entry));
                            }
                            if ui.button("Copy syntax").clicked() {
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
        action
    }
}
