use super::{CalculationPadDialog, HelpEffects, InspectorCaret, PadHostIdentity, pad_viewport_id};
use crate::calculation_pad::reference::{self, ReferenceCategory, ReferenceId};
use crate::calculation_pad::{CalculationExample, EvaluationBasis};
use eframe::egui;
use reference::insertion::{PreparedExampleInsertion, prepare_example_insertion};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EntryFilter {
    #[default]
    All,
    Functions,
    Topics,
}

impl EntryFilter {
    const ALL: [Self; 3] = [Self::All, Self::Functions, Self::Topics];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All types",
            Self::Functions => "Functions",
            Self::Topics => "Language and usage",
        }
    }

    fn includes(self, entry: ReferenceId) -> bool {
        self == Self::All
            || matches!(
                (self, entry),
                (Self::Functions, ReferenceId::Call(_)) | (Self::Topics, ReferenceId::Topic(_))
            )
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DetailLocation {
    entry: ReferenceId,
    scroll: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ReferenceView {
    pub(super) api_visible: bool,
    query: String,
    category: Option<ReferenceCategory>,
    filter: EntryFilter,
    results_scroll: f32,
    last_selected: Option<ReferenceId>,
    details: Vec<DetailLocation>,
}

impl ReferenceView {
    pub(super) fn page(&self) -> (bool, Option<ReferenceId>) {
        (self.api_visible, self.details.last().map(|page| page.entry))
    }

    fn open(&mut self, entry: ReferenceId) {
        if self.details.is_empty() {
            self.last_selected = Some(entry);
        }
        if self.details.len() == 32 {
            self.details.remove(0);
        }
        self.details.push(DetailLocation { entry, scroll: 0.0 });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InsertAnchor {
    caret: InspectorCaret,
    basis: EvaluationBasis,
    host: PadHostIdentity,
}

pub(super) enum InsertAction {
    Prepare {
        anchor: InsertAnchor,
        example: CalculationExample,
    },
    Confirm {
        anchor: InsertAnchor,
        sample: &'static str,
    },
    Cancel,
}

pub(super) struct PendingExampleInsert {
    anchor: InsertAnchor,
    sample: &'static str,
    title: &'static str,
    prepared: PreparedExampleInsertion,
}

fn click_with_focus_reveal(response: egui::Response) -> bool {
    if response.gained_focus() {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
    response.clicked()
}

impl CalculationPadDialog {
    pub(super) fn reference_contents(
        &mut self,
        ui: &mut egui::Ui,
        effects: &mut HelpEffects,
        view: &mut ReferenceView,
    ) {
        if let Some(pending) = &self.pending_reference_insert {
            ui.group(|ui| {
                ui.strong("Review insertion");
                ui.label(pending.title);
                ui.collapsing("Example source", |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(pending.sample).monospace()).wrap(),
                    );
                });
                if !pending.prepared.conflicting_names.is_empty() {
                    ui.label("Names already used in the draft");
                    ui.label(pending.prepared.conflicting_names.join(", "));
                }
                if pending.prepared.shifted_row_references {
                    ui.label("Existing physical row references may point at different results.");
                }
                let valid = self.reference_anchor().as_ref() == Some(&pending.anchor);
                if !valid {
                    ui.label("The Pad changed. Choose the insertion point again.");
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(valid, egui::Button::new("Confirm Insert"))
                        .clicked()
                    {
                        effects.reference_insert = Some(InsertAction::Confirm {
                            anchor: pending.anchor.clone(),
                            sample: pending.sample,
                        });
                    }
                    if ui.button("Cancel insertion").clicked() {
                        effects.reference_insert = Some(InsertAction::Cancel);
                    }
                });
            });
        }
        if let Some(message) = &self.reference_feedback {
            ui.label(message);
        }
        let Some(current) = view.details.last().cloned() else {
            self.reference_results_ui(ui, view);
            return;
        };
        let Some(entry) = reference::entry(current.entry) else {
            view.details.clear();
            return;
        };
        let mut back = false;
        let mut results = false;
        ui.horizontal_wrapped(|ui| {
            results = ui.button("Back to results").clicked();
            if view.details.len() > 1 {
                back = ui.button("Back to previous entry").clicked();
            }
        });
        if results {
            view.details.clear();
            return;
        }
        if back {
            view.details.pop();
            return;
        }

        let mut related = None;
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("calculation-reference-detail", current.entry))
            .vertical_scroll_offset(current.scroll)
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.heading(entry.id.name());
                ui.label(entry.category.label());
                ui.add(egui::Label::new(egui::RichText::new(entry.signature()).monospace()).wrap());
                ui.label(entry.summary);
                for (title, description) in [
                    ("Parameters", entry.parameters),
                    ("Returns", entry.returns),
                    ("Limits and errors", entry.limits),
                ] {
                    ui.add_space(6.0);
                    ui.strong(title);
                    ui.label(description);
                }
                if !entry.aliases.is_empty() {
                    ui.strong("Other spellings");
                    ui.label(entry.aliases.join(", "));
                }
                let units = entry.unit_definitions().collect::<Vec<_>>();
                if !units.is_empty() {
                    ui.collapsing("Accepted units and aliases", |ui| {
                        for unit in units {
                            ui.group(|ui| {
                                ui.strong(unit.symbol);
                                if !unit.aliases.is_empty() {
                                    ui.label(unit.aliases.join(", "));
                                }
                                if !unit.case_sensitive_aliases.is_empty() {
                                    ui.strong("Case-sensitive aliases");
                                    ui.label(unit.case_sensitive_aliases.join(", "));
                                }
                            });
                        }
                    });
                }
                ui.separator();
                let anchor = self.reference_anchor();
                for (index, example) in std::iter::once(&entry.example)
                    .chain(entry.variants)
                    .enumerate()
                {
                    ui.push_id(index, |ui| {
                        let title = if index == 0 { "Example" } else { example.title };
                        ui.strong(title);
                        ui.add(
                            egui::Label::new(egui::RichText::new(example.source).monospace())
                                .wrap(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.strong("Expected result");
                            ui.label(example.result_text());
                        });
                        ui.horizontal_wrapped(|ui| {
                            if click_with_focus_reveal(ui.button("Copy Example")) {
                                effects.copy = Some(example.source.to_owned());
                            }
                            let insert = ui.add_enabled(
                                anchor.is_some() && self.pending_reference_insert.is_none(),
                                egui::Button::new("Insert Example into Pad"),
                            );
                            if click_with_focus_reveal(insert)
                                && let Some(anchor) = anchor.clone()
                            {
                                effects.reference_insert = Some(InsertAction::Prepare {
                                    anchor,
                                    example: *example,
                                });
                            }
                        });
                    });
                }
                if anchor.is_none() {
                    ui.weak("Place the caret in the Pad source to enable insertion.");
                }
                if !entry.related.is_empty() {
                    ui.add_space(6.0);
                    ui.strong("Related entries");
                    ui.horizontal_wrapped(|ui| {
                        for &id in entry.related {
                            if click_with_focus_reveal(ui.button(id.name())) {
                                related = Some(id);
                            }
                        }
                    });
                }
            });
        if let Some(page) = view.details.last_mut() {
            page.scroll = scroll.state.offset.y;
        }
        if let Some(id) = related {
            view.open(id);
        }
    }

    fn reference_results_ui(&self, ui: &mut egui::Ui, view: &mut ReferenceView) {
        let old_query = view.query.clone();
        let old_category = view.category;
        let old_filter = view.filter;
        let mut enter = false;
        ui.horizontal_wrapped(|ui| {
            let search_id = ui.make_persistent_id("api-reference-search");
            enter = ui.memory(|memory| memory.has_focus(search_id))
                && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let search = ui.add(
                egui::TextEdit::singleline(&mut view.query)
                    .id(search_id)
                    .desired_width((ui.available_width() - 70.0).max(80.0))
                    .char_limit(128)
                    .hint_text("Name, type or concept"),
            );
            enter &= search.enabled();
            if ui.button("Clear search").clicked() {
                view.query.clear();
            }
        });
        let filters = format!(
            "Filters · {} · {}",
            view.category
                .map_or("All categories", ReferenceCategory::label),
            view.filter.label()
        );
        egui::CollapsingHeader::new(filters)
            .id_salt("api-reference-filters")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut view.category, None, "All categories");
                    for category in ReferenceCategory::ALL {
                        ui.selectable_value(&mut view.category, Some(category), category.label());
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for filter in EntryFilter::ALL {
                        ui.selectable_value(&mut view.filter, filter, filter.label());
                    }
                });
            });
        if view.query != old_query || view.category != old_category || view.filter != old_filter {
            view.results_scroll = 0.0;
            view.last_selected = None;
        }
        let entries = reference::search(&view.query, view.category);
        let mut entries = entries
            .into_iter()
            .filter(|entry| view.filter.includes(entry.id))
            .peekable();
        if enter && let Some(entry) = entries.peek() {
            view.open(entry.id);
            return;
        }
        if entries.peek().is_none() {
            ui.label("No matching API entries.");
            return;
        }
        let mut selected = None;
        let scroll = egui::ScrollArea::vertical()
            .id_salt("api-reference-results")
            .vertical_scroll_offset(view.results_scroll)
            .max_height(ui.available_height())
            .show(ui, |ui| {
                for entry in entries {
                    ui.push_id(entry.id, |ui| {
                        let row = ui
                            .selectable_label(
                                view.last_selected == Some(entry.id),
                                entry.signature(),
                            )
                            .on_hover_text(entry.summary);
                        if click_with_focus_reveal(row) {
                            selected = Some(entry.id);
                        }
                    });
                }
            });
        view.results_scroll = scroll.state.offset.y;
        if let Some(id) = selected {
            view.open(id);
        }
    }
}

impl CalculationPadDialog {
    pub(super) fn reference_anchor(&self) -> Option<InsertAnchor> {
        if !self.open || self.session.pending_transition().is_some() {
            return None;
        }
        let basis = self.session.evaluation_basis()?;
        let caret = self.inspector_caret.as_ref()?;
        if caret.incarnation != basis.incarnation
            || caret.document != basis.document
            || caret.revision != basis.revision
        {
            return None;
        }
        self.session.source()?.get(caret.range.clone())?;
        Some(InsertAnchor {
            caret: caret.clone(),
            basis,
            host: self.host_identity(),
        })
    }

    fn reference_anchor_is_safe(&self, anchor: &InsertAnchor) -> bool {
        if self.reference_anchor().as_ref() != Some(anchor) {
            return false;
        }
        if self.folds.collapsed.is_empty() {
            return true;
        }
        let Some(source) = self.session.source() else {
            return false;
        };
        let ids = self.session.line_ids().unwrap_or_default();
        let range = &anchor.caret.range;
        !super::editor::fold_regions(source).iter().any(|region| {
            ids.get(region.header)
                .is_some_and(|id| self.folds.collapsed.contains(id))
                && ((range.is_empty() && region.hidden.contains(&range.start))
                    || (region.hidden.start < range.end && range.start < region.hidden.end))
        })
    }

    pub(super) fn complete_reference_insert(
        &mut self,
        ctx: &egui::Context,
        action: Option<InsertAction>,
    ) {
        match action {
            Some(InsertAction::Cancel) => {
                self.pending_reference_insert = None;
                self.reference_feedback = None;
            }
            Some(InsertAction::Confirm { anchor, sample }) => {
                if self
                    .pending_reference_insert
                    .as_ref()
                    .is_some_and(|pending| pending.anchor == anchor && pending.sample == sample)
                    && let Some(pending) = self.pending_reference_insert.take()
                {
                    self.apply_reference_insert(ctx, pending);
                }
            }
            Some(InsertAction::Prepare { anchor, example }) => {
                self.pending_reference_insert = None;
                self.reference_feedback = None;
                if !self.reference_anchor_is_safe(&anchor) {
                    self.reference_feedback =
                        Some("Choose a current insertion point outside folded text.".into());
                    return;
                }
                let source = self.session.source().unwrap_or_default();
                match prepare_example_insertion(source, anchor.caret.range.clone(), example.source)
                {
                    Ok(prepared) => {
                        let review = !prepared.conflicting_names.is_empty()
                            || prepared.shifted_row_references;
                        let pending = PendingExampleInsert {
                            anchor,
                            sample: example.source,
                            title: example.title,
                            prepared,
                        };
                        if review {
                            self.pending_reference_insert = Some(pending);
                        } else {
                            self.apply_reference_insert(ctx, pending);
                        }
                    }
                    Err(reason) => self.reference_feedback = Some(reason.to_string()),
                }
            }
            None => {
                if self
                    .pending_reference_insert
                    .as_ref()
                    .is_some_and(|pending| !self.reference_anchor_is_safe(&pending.anchor))
                {
                    self.pending_reference_insert = None;
                    self.reference_feedback =
                        Some("The Pad changed. Choose the insertion point again.".into());
                    ctx.request_repaint();
                }
            }
        }
    }

    fn apply_reference_insert(&mut self, ctx: &egui::Context, pending: PendingExampleInsert) {
        if !self.reference_anchor_is_safe(&pending.anchor) {
            self.reference_feedback =
                Some("The Pad changed. Choose the insertion point again.".into());
            return;
        }
        let old_source = (!self.pad_view.inputs.is_empty())
            .then(|| self.session.source().unwrap_or_default().to_owned());
        let old_ids = old_source
            .as_ref()
            .map(|_| self.session.line_ids().unwrap_or_default().to_vec());
        match self
            .session
            .apply_reference_example(pending.anchor.basis, pending.prepared)
        {
            Ok(true) => {
                self.reconcile_input_selections(old_source.as_deref(), old_ids.as_deref(), false);
                self.persist_view_state_in_session();
                self.source_highlight = None;
                self.dependency_navigation = None;
                self.selected_result = None;
                self.active_source_line = None;
                self.pending_result_scroll = None;
                self.inspector_caret = None;
                self.schedule_recovery_snapshot(ctx);
                self.sync_inspector_context();
                self.reference_feedback = Some(
                    "Example inserted into the Pad. Use Undo in the source editor to remove it."
                        .into(),
                );
                ctx.request_repaint_of(egui::ViewportId::ROOT);
                ctx.request_repaint_of(pad_viewport_id());
            }
            Ok(false) => {
                self.reference_feedback = Some("The example matches the selected source.".into())
            }
            Err(error) => self.reference_feedback = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        view: &mut ReferenceView,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 760.0),
            )),
            events,
            focused: true,
            ..Default::default()
        };
        let mut effects = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        let output = ctx.run_ui(input, |ui| {
            dialog.reference_contents(ui, &mut effects, view)
        });
        fn collect(shape: &egui::epaint::Shape, labels: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    let center = text.galley.rect.center() + text.pos.to_vec2();
                    labels.push((text.galley.job.text.clone(), center));
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for child in shapes {
                        collect(child, labels);
                    }
                }
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut labels);
        }
        output.drop_without_applying_deltas();
        labels
    }

    fn click_label(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        view: &mut ReferenceView,
        label: &str,
    ) {
        let labels = render(dialog, ctx, view, Vec::new());
        let pos = labels
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label} in {labels:?}"))
            .1;
        let event = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        render(
            dialog,
            ctx,
            view,
            vec![egui::Event::PointerMoved(pos), event(true)],
        );
        render(dialog, ctx, view, vec![event(false)]);
    }

    #[test]
    fn reference_api_keyboard_search_and_related_navigation_preserve_the_pad() {
        let directory = tempfile::tempdir().unwrap();
        let root =
            crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut dialog = CalculationPadDialog::new(&root);
        dialog.open();
        let ctx = egui::Context::default();
        let mut view = ReferenceView::default();
        click_label(&mut dialog, &ctx, &mut view, "Name, type or concept");
        render(
            &mut dialog,
            &ctx,
            &mut view,
            vec![egui::Event::Text("remove last element".into())],
        );
        render(
            &mut dialog,
            &ctx,
            &mut view,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(view.details.last().unwrap().entry, ReferenceId::Call("pop"));
        click_label(&mut dialog, &ctx, &mut view, "last");
        assert_eq!(
            view.details.last().unwrap().entry,
            ReferenceId::Call("last")
        );
        click_label(&mut dialog, &ctx, &mut view, "Back to previous entry");
        assert_eq!(view.details.last().unwrap().entry, ReferenceId::Call("pop"));
        click_label(&mut dialog, &ctx, &mut view, "Back to results");
        assert!(view.details.is_empty());
        assert_eq!(view.query, "remove last element");
        let mut all_results = ReferenceView::default();
        click_label(&mut dialog, &ctx, &mut all_results, "Name, type or concept");
        for _ in 0..55 {
            for pressed in [true, false] {
                render(
                    &mut dialog,
                    &ctx,
                    &mut all_results,
                    vec![egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
        }
        assert!(
            all_results.results_scroll > 0.0,
            "Tab must reveal results below the initial viewport"
        );
        assert_eq!(dialog.session.source(), Some(""));
        assert!(!dialog.session.can_undo());
    }
}
