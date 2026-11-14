//! Read-only dependency companion for the active Calculation Pad.
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::UtilityPresentation;
use crate::calculation_pad::{
    DependencySnapshot, DocumentId, DocumentIncarnation, LineId, LineOutcome,
};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DependencyNavigation {
    pub document: DocumentId,
    pub incarnation: DocumentIncarnation,
    pub line: LineId,
}
#[derive(Default)]
pub(crate) struct CalculationDependencyDialog {
    pub(super) open: bool,
    epoch: u64,
    close_requested: bool,
    selected: Option<(DocumentId, DocumentIncarnation, LineId)>,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect, egui::Rect)>,
}
pub(super) struct DependencyRuntime {
    pub snapshot: Option<Arc<DependencySnapshot>>,
    pub pad_host: Option<super::utility_host::HostIdentity>,
    pub pinned: bool,
}
#[derive(Default)]
pub(super) struct DependencyActions {
    pub basis: Option<(DocumentId, DocumentIncarnation)>,
    pub pad_host: Option<super::utility_host::HostIdentity>,
    pub host_request: Option<UtilityPresentation>,
    pub close: bool,
    pub selected: Option<LineId>,
    pub navigate: Option<DependencyNavigation>,
}

impl CalculationDependencyDialog {
    pub(super) fn open(&mut self, seed: Option<(DocumentId, DocumentIncarnation, LineId)>) {
        if !self.open {
            self.epoch = self.epoch.wrapping_add(1);
            self.selected = seed;
            self.open = true;
            self.close_requested = false;
        }
    }
    pub(super) fn close(&mut self) {
        self.open = false;
        self.close_requested = false;
        self.selected = None;
    }
    pub(super) fn request_close(&mut self) {
        self.close_requested = true;
    }
    #[cfg(test)]
    pub(super) fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect)> {
        self.test_controls
            .iter()
            .find(|(key, _, _)| key == name)
            .map(|(_, rect, clip)| (*rect, *clip))
    }
    #[cfg(test)]
    pub(super) fn test_selected(&self) -> Option<LineId> {
        self.selected.map(|(_, _, id)| id)
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(super) fn commit(
        &mut self,
        actions: DependencyActions,
        snapshot: Option<&DependencySnapshot>,
    ) -> Option<DependencyNavigation> {
        if actions.host_request.is_some() || actions.close {
            return None;
        }
        if let Some(id) = actions.selected
            && snapshot.is_some_and(|s| {
                actions.basis == Some((s.document_id, s.incarnation))
                    && s.rows.iter().any(|row| row.id == id)
            })
        {
            self.selected = snapshot.map(|s| (s.document_id, s.incarnation, id));
        }
        if self.selected.is_some_and(|(doc, inc, id)| {
            !snapshot.is_some_and(|s| {
                s.document_id == doc
                    && s.incarnation == inc
                    && s.rows.iter().any(|row| row.id == id)
            })
        }) {
            self.selected = None;
        }
        actions.navigate.filter(|nav| {
            snapshot.is_some_and(|s| {
                s.document_id == nav.document
                    && s.incarnation == nav.incarnation
                    && s.rows.iter().any(|row| row.id == nav.line)
            })
        })
    }
    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        runtime: &DependencyRuntime,
        owner: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> DependencyActions {
        #[cfg(test)]
        self.test_controls.clear();
        let mut actions = DependencyActions {
            basis: runtime
                .snapshot
                .as_ref()
                .map(|s| (s.document_id, s.incarnation)),
            pad_host: runtime.pad_host,
            ..Default::default()
        };
        let mut window_open = self.open;
        let mut body = |ui: &mut egui::Ui| {
            let active =
                owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            let (button, next) = super::utility_host::presentation_button(ui, presentation, active);
            #[cfg(test)]
            self.test_controls
                .push(("host".into(), button.rect, ui.clip_rect()));
            if active && button.clicked() {
                actions.host_request = Some(next);
            }
            let close = ui.add_enabled(active && !runtime.pinned, egui::Button::new("Close"));
            #[cfg(test)]
            self.test_controls
                .push(("close".into(), close.rect, ui.clip_rect()));
            if close.clicked() {
                actions.close = true;
            }
            ui.add_enabled_ui(active, |ui| {
                let Some(snapshot) = runtime.snapshot.as_deref() else {
                    ui.label("Calculation Pad is unavailable. Open a Pad document to browse dependencies.");
                    return;
                };
                if let Some(diagnostic) = &snapshot.diagnostic {
                    ui.colored_label(ui.visuals().error_fg_color, diagnostic.to_string());
                }
                let selected = self.selected.and_then(|(doc, inc, id)| {
                    (snapshot.document_id == doc && snapshot.incarnation == inc
                        && snapshot.rows.iter().any(|row| row.id == id)).then_some(id)
                });
                if selected.is_none() {
                    ui.label("Choose a source line to inspect its relationships.");
                }
                egui::CollapsingHeader::new("Choose source line")
                    .default_open(selected.is_none())
                    .show(ui, |ui| {
                        for row in &snapshot.rows {
                            let response = ui.selectable_label(selected == Some(row.id), row_label(row))
                                .on_hover_text(&row.source);
                            #[cfg(test)]
                            self.test_controls.push((format!("row/{}", row.line_index + 1), response.rect, ui.clip_rect()));
                            if response.clicked() { actions.selected = Some(row.id); }
                        }
                    });
                if let Some(row) = selected.and_then(|id| snapshot.rows.iter().find(|row| row.id == id)) {
                    ui.strong(row_label(row)).on_hover_text(&row.source);
                    if let Some(iteration) = &row.loop_iteration {
                        let path = iteration.iter().map(usize::to_string).collect::<Vec<_>>().join(".");
                        ui.weak(format!("Source-line projection of the last execution, iteration {path}. An input may refer to an earlier iteration."));
                    }
                    ui.add(egui::Label::new(egui::RichText::new(&row.source).monospace()).truncate())
                        .on_hover_text(&row.source);
                    match &row.outcome {
                        LineOutcome::Value(value) => { ui.label(describe_value(value)); }
                        LineOutcome::Error(error) => { ui.colored_label(ui.visuals().error_fg_color, error.to_string()); }
                        LineOutcome::Blank => { ui.weak("Blank line"); }
                        LineOutcome::Comment => { ui.weak("Comment"); }
                    }
                    let go = ui.button("Go to source");
                    #[cfg(test)]
                    self.test_controls.push(("go".into(), go.rect, ui.clip_rect()));
                    if go.clicked() {
                        actions.navigate = Some(DependencyNavigation {
                            document: snapshot.document_id, incarnation: snapshot.incarnation, line: row.id,
                        });
                    }
                    for (title, direct, upstream) in [
                        ("Upstream inputs", &row.dependencies, true),
                        ("Downstream dependents", &row.dependents, false),
                    ] {
                        ui.collapsing(title, |ui| {
                            ui.label("Direct");
                            relation_rows(ui, snapshot, direct, &mut actions);
                            ui.collapsing("Transitive", |ui| {
                                let transitive = if upstream { snapshot.upstream(row.id) } else { snapshot.downstream(row.id) };
                                relation_rows(ui, snapshot, transitive.as_deref().unwrap_or_default(), &mut actions);
                            });
                        });
                    }
                }
            });
        };
        if let Some(ui) = child {
            egui::ScrollArea::vertical()
                .id_salt("dependency-contents")
                .show(ui, |ui| body(ui));
        } else {
            egui::Window::new("Dependency Explorer")
                .id(egui::Id::new("calculation-dependencies-window"))
                .default_size([560.0, 500.0])
                .min_size([320.0, 240.0])
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    "calculation-pad-dependencies",
                    egui::Id::new("calculation-dependencies-window"),
                    Some(&mut window_open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(
                            560.0, 500.0,
                        )),
                        egui::vec2(320.0, 240.0),
                        [None, None],
                    ),
                    |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| body(ui));
                    },
                );
        }
        actions.close |= (!window_open || self.close_requested) && !runtime.pinned;
        actions
    }
}
fn describe_value(value: &crate::calculation_pad::CalculationValue) -> String {
    use crate::calculation_pad::CalculationValue;
    let kind = match value {
        CalculationValue::Scalar(_) => "Scalar".to_owned(),
        CalculationValue::Boolean(_) => "Boolean".to_owned(),
        CalculationValue::Sequence(_) => "Sequence".to_owned(),
        CalculationValue::Vector(_) => "Vector".to_owned(),
        CalculationValue::Integer(integer) => integer.mode().type_name().to_owned(),
        CalculationValue::DimensionalQuantity(_) => "Quantity".to_owned(),
        CalculationValue::PhysicalQuantity { .. } => "Physical quantity".to_owned(),
        CalculationValue::ExactBase { base, .. } => format!("{base:?}"),
    };
    format!(
        "{kind}: {}",
        value
            .formatted_result()
            .unwrap_or_else(|| "Unavailable".into())
    )
}
fn row_label(row: &crate::calculation_pad::DependencyRow) -> String {
    let text = row.assignment_name.as_deref().unwrap_or_else(|| {
        if row.source.trim().is_empty() {
            "(blank)"
        } else {
            row.source.trim()
        }
    });
    let mut chars = text.chars();
    let excerpt: String = chars.by_ref().take(64).collect();
    format!(
        "r{} · {}{}",
        row.line_index + 1,
        excerpt,
        if chars.next().is_some() { "…" } else { "" }
    )
}
fn relation_rows(
    ui: &mut egui::Ui,
    snapshot: &DependencySnapshot,
    rows: &[LineId],
    actions: &mut DependencyActions,
) {
    if rows.is_empty() {
        ui.weak("None");
    }
    for id in rows {
        if let Some(row) = snapshot.rows.iter().find(|row| row.id == *id)
            && ui
                .button(row_label(row))
                .on_hover_text(&row.source)
                .clicked()
        {
            actions.selected = Some(*id);
            actions.navigate = Some(DependencyNavigation {
                document: snapshot.document_id,
                incarnation: snapshot.incarnation,
                line: *id,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependency_companion_rejects_deleted_and_reopened_source_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(dir.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("α = 2\nα + 1").unwrap();
        session.save_as("dependencies").unwrap();
        session.evaluation();
        let first = session.dependency_snapshot().unwrap();
        let old = DependencyNavigation {
            document: first.document_id,
            incarnation: first.incarnation,
            line: first.rows[1].id,
        };
        let mut dialog = CalculationDependencyDialog::default();
        dialog.open(Some((old.document, old.incarnation, old.line)));
        session.replace_source("α = 2").unwrap();
        session.evaluation();
        let deleted = session.dependency_snapshot().unwrap();
        let actions = DependencyActions {
            basis: Some((old.document, old.incarnation)),
            navigate: Some(old),
            ..Default::default()
        };
        assert!(dialog.commit(actions, Some(&deleted)).is_none());
        assert!(dialog.selected.is_none());
        session.request_open(old.document).unwrap();
        session
            .resolve_pending(crate::calculation_pad::TransitionDecision::Discard)
            .unwrap();
        session.evaluation();
        let reopened = session.dependency_snapshot().unwrap();
        assert_ne!(reopened.incarnation, old.incarnation);
        let actions = DependencyActions {
            basis: Some((old.document, old.incarnation)),
            selected: Some(old.line),
            navigate: Some(old),
            ..Default::default()
        };
        assert!(dialog.commit(actions, Some(&reopened)).is_none());
        assert!(dialog.selected.is_none());
    }

    #[test]
    fn dependency_companion_narrow_large_text_keeps_chooser_and_close_reachable() {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(dir.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session
            .replace_source(format!("# {}\nα = 2", "long Unicode 名 ".repeat(100)))
            .unwrap();
        session.evaluation();
        let runtime = DependencyRuntime {
            snapshot: session.dependency_snapshot(),
            pad_host: None,
            pinned: false,
        };
        let mut dialog = CalculationDependencyDialog::default();
        dialog.open(None);
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.size *= 1.25;
            }
        });
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 480.0),
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                let local_ctx = ui.ctx().clone();
                dialog.render_host(
                    &local_ctx,
                    &runtime,
                    true,
                    true,
                    Some(ui),
                    UtilityPresentation::Docked,
                );
            });
            output.drop_without_applying_deltas();
        }
        for name in ["row/1", "row/2", "close"] {
            let (rect, clip) = dialog.test_control(name).expect("control drawn");
            assert!(clip.contains_rect(rect), "{name}: {rect:?} vs {clip:?}");
        }
    }
}
