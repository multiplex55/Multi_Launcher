//! Read-only dependency companion for the active Calculation Pad.
use super::jump_mode::{
    TargetKind,
    ui::{JumpModeScope, register_and_apply_target},
};
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::UtilityPresentation;
use crate::calculation_pad::{
    DependencySnapshot, DocumentId, DocumentIncarnation, EvaluationStatus, LineId, LineOutcome,
};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DependencyNavigation {
    pub document: DocumentId,
    pub incarnation: DocumentIncarnation,
    pub line: LineId,
    pub basis: Option<crate::calculation_pad::EvaluationBasis>,
}
#[derive(Default)]
pub(crate) struct CalculationDependencyDialog {
    pub(super) open: bool,
    epoch: u64,
    close_requested: bool,
    selected: Option<(DocumentId, DocumentIncarnation, LineId)>,
    jump_generation: u64,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_last_source: Option<egui::Id>,
}
pub(super) struct DependencyRuntime {
    pub snapshot: Option<Arc<DependencySnapshot>>,
    pub identity: Option<(DocumentId, DocumentIncarnation)>,
    pub status: EvaluationStatus,
    pub pad_host: Option<super::utility_host::HostIdentity>,
    pub pinned: bool,
}
#[derive(Default)]
pub(super) struct DependencyActions {
    pub basis: Option<crate::calculation_pad::EvaluationBasis>,
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
    pub(super) fn sync_jump_generation(&mut self, runtime: &DependencyRuntime) {
        let basis = runtime.snapshot.as_ref().map(|snapshot| {
            (
                snapshot.basis.document,
                snapshot.basis.incarnation,
                snapshot.basis.revision,
                snapshot.basis.generation,
            )
        });
        self.jump_generation = egui::Id::new((
            "dependency",
            self.epoch,
            runtime.identity,
            basis,
            self.selected,
        ))
        .value();
    }
    pub(super) fn jump_generation(&self) -> u64 {
        self.jump_generation
    }

    pub(super) fn commit(
        &mut self,
        actions: DependencyActions,
        snapshot: Option<&DependencySnapshot>,
        identity: Option<(DocumentId, DocumentIncarnation)>,
        status: &EvaluationStatus,
    ) -> Option<(DependencyNavigation, u64)> {
        if actions.host_request.is_some() || actions.close {
            return None;
        }
        if let Some(id) = actions.selected
            && snapshot.is_some_and(|s| {
                actions.basis == Some(s.basis) && s.rows.iter().any(|row| row.id == id)
            })
        {
            self.selected = snapshot.map(|s| (s.document_id, s.incarnation, id));
        }
        if self.selected.is_some_and(|(doc, inc, id)| {
            !snapshot.is_some_and(|s| {
                s.document_id == doc
                    && s.incarnation == inc
                    && s.rows.iter().any(|row| row.id == id)
            }) && !(identity == Some((doc, inc))
                && !matches!(status, EvaluationStatus::Ready | EvaluationStatus::Inactive))
        }) {
            self.selected = None;
        }
        let basis = actions.basis;
        actions
            .navigate
            .filter(|nav| {
                snapshot.is_some_and(|s| {
                    basis == Some(s.basis)
                        && nav.basis == Some(s.basis)
                        && s.document_id == nav.document
                        && s.incarnation == nav.incarnation
                        && s.rows.iter().any(|row| row.id == nav.line)
                })
            })
            .and_then(|navigation| basis.map(|basis| (navigation, basis.revision)))
    }
    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        runtime: &DependencyRuntime,
        owner: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
        scope: Option<&JumpModeScope>,
    ) -> DependencyActions {
        #[cfg(test)]
        {
            self.test_controls.clear();
            self.test_last_source = None;
        }
        let mut actions = DependencyActions {
            basis: runtime.snapshot.as_ref().map(|s| s.basis),
            pad_host: runtime.pad_host,
            ..Default::default()
        };
        let mut window_open = self.open;
        let mut order = 0;
        let mut body = |ui: &mut egui::Ui| {
            let active =
                owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            let (button, next) = super::utility_host::presentation_button(ui, presentation, owner);
            dependency_target(
                scope,
                ui,
                &button,
                "presentation",
                "Change window presentation",
                TargetKind::Button,
                &mut order,
            );
            #[cfg(test)]
            self.test_controls
                .push(("host".into(), button.rect, ui.clip_rect()));
            if active && button.clicked() {
                actions.host_request = Some(next);
            }
            let close = ui.add_enabled(owner && !runtime.pinned, egui::Button::new("Close"));
            dependency_target(
                scope,
                ui,
                &close,
                "close",
                "Close",
                TargetKind::Button,
                &mut order,
            );
            #[cfg(test)]
            self.test_controls
                .push(("close".into(), close.rect, ui.clip_rect()));
            if close.clicked() {
                actions.close = true;
            }
            ui.add_enabled_ui(owner, |ui| {
                let Some(snapshot) = runtime.snapshot.as_deref() else {
                    match &runtime.status {
                        EvaluationStatus::Pending if runtime.identity.is_some() => { ui.label("Calculation is pending. Dependencies will appear when results are ready."); }
                        EvaluationStatus::NotCalculated | EvaluationStatus::Outdated | EvaluationStatus::Canceled if runtime.identity.is_some() => { ui.label("Calculate the current document to browse dependencies."); }
                        EvaluationStatus::Failed(message) if runtime.identity.is_some() => { ui.colored_label(ui.visuals().error_fg_color, message); }
                        _ => { ui.label("Calculation Pad is unavailable. Open a Pad document to browse dependencies."); }
                    }
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
                let chooser = egui::CollapsingHeader::new("Choose source line")
                    .default_open(selected.is_none())
                    .show(ui, |ui| {
                        for row in &snapshot.rows {
                            let response = ui.push_id(("dependency-row", snapshot.document_id, snapshot.incarnation, row.id), |ui| ui.selectable_label(selected == Some(row.id), row_label(row))).inner
                                .on_hover_text(&row.source);
                            dependency_target(scope, ui, &response, format!("row/{}", row.id.get()), row_label(row), TargetKind::Selectable, &mut order);
                            #[cfg(test)]
                            if snapshot.rows.last().is_some_and(|last| last.id == row.id) {
                                self.test_last_source = Some(response.id);
                            }
                            #[cfg(test)]
                            self.test_controls.push((format!("row/{}", row.line_index + 1), response.rect, ui.clip_rect()));
                            if response.clicked() { actions.selected = Some(row.id); }
                        }
                    });
                dependency_target(scope, ui, &chooser.header_response, "chooser", "Choose source line", TargetKind::Button, &mut order);
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
                        LineOutcome::Definition => { ui.weak("Function definition"); }
                        LineOutcome::Control => { ui.weak("Control statement"); }
                        LineOutcome::Continuation { owner_line } => { ui.weak(format!("Continuation of line {}", owner_line + 1)); }
                    }
                    let go = ui.button("Go to source");
                    dependency_target(scope, ui, &go, "go", "Go to source", TargetKind::Button, &mut order);
                    #[cfg(test)]
                    self.test_controls.push(("go".into(), go.rect, ui.clip_rect()));
                    if go.clicked() {
                        actions.navigate = Some(DependencyNavigation {
                            document: snapshot.document_id, incarnation: snapshot.incarnation, line: row.id,
                            basis: Some(snapshot.basis),
                        });
                    }
                    for (group, title, direct, upstream) in [
                        ("upstream", "Upstream inputs", &row.dependencies, true),
                        ("downstream", "Downstream dependents", &row.dependents, false),
                    ] {
                        let relations = ui.collapsing(title, |ui| {
                            ui.label("Direct");
                            relation_rows(ui, snapshot, direct, &mut actions, scope, &mut order, &format!("{group}/direct"));
                            let transitive = ui.collapsing("Transitive", |ui| {
                                let rows = if upstream { snapshot.upstream(row.id) } else { snapshot.downstream(row.id) };
                                relation_rows(ui, snapshot, rows.as_deref().unwrap_or_default(), &mut actions, scope, &mut order, &format!("{group}/transitive"));
                            });
                            dependency_target(scope, ui, &transitive.header_response, format!("{group}/transitive/header"), format!("Transitive {title}"), TargetKind::Button, &mut order);
                        });
                        dependency_target(scope, ui, &relations.header_response, format!("{group}/header"), title, TargetKind::Button, &mut order);
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
        CalculationValue::ApproximateScalar(_) => "Approximate scalar".to_owned(),
        CalculationValue::Exact(_) => "Exact".to_owned(),
        CalculationValue::Calendar(value) => value.type_label().to_owned(),
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
fn dependency_target(
    scope: Option<&JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: TargetKind,
    order: &mut u64,
) {
    register_and_apply_target(scope, ui, response, identity, label, kind, order);
    if response.enabled()
        && response.gained_focus()
        && ui.is_visible()
        && !ui.is_sizing_pass()
        && !ui.ctx().will_discard()
        && !ui.clip_rect().contains_rect(response.rect)
    {
        response.scroll_to_me(None);
    }
}

fn relation_rows(
    ui: &mut egui::Ui,
    snapshot: &DependencySnapshot,
    rows: &[LineId],
    actions: &mut DependencyActions,
    scope: Option<&JumpModeScope>,
    order: &mut u64,
    group: &str,
) {
    if rows.is_empty() {
        ui.weak("None");
    }
    for id in rows {
        if let Some(row) = snapshot.rows.iter().find(|row| row.id == *id) {
            let response = ui
                .push_id((group, *id), |ui| ui.button(row_label(row)))
                .inner
                .on_hover_text(&row.source);
            dependency_target(
                scope,
                ui,
                &response,
                format!("{group}/{}", id.get()),
                row_label(row),
                TargetKind::Button,
                order,
            );
            if response.clicked() {
                actions.selected = Some(*id);
                actions.navigate = Some(DependencyNavigation {
                    document: snapshot.document_id,
                    incarnation: snapshot.incarnation,
                    line: *id,
                    basis: Some(snapshot.basis),
                });
            }
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
            basis: Some(first.basis),
        };
        let mut dialog = CalculationDependencyDialog::default();
        dialog.open(Some((old.document, old.incarnation, old.line)));
        session.replace_source("α = 2").unwrap();
        session.evaluation();
        let deleted = session.dependency_snapshot().unwrap();
        let actions = DependencyActions {
            basis: Some(first.basis),
            navigate: Some(old),
            ..Default::default()
        };
        assert!(
            dialog
                .commit(
                    actions,
                    Some(&deleted),
                    Some((deleted.document_id, deleted.incarnation)),
                    &EvaluationStatus::Ready
                )
                .is_none()
        );
        assert!(dialog.selected.is_none());
        session.request_open(old.document).unwrap();
        session
            .resolve_pending(crate::calculation_pad::TransitionDecision::Discard)
            .unwrap();
        session.evaluation();
        let reopened = session.dependency_snapshot().unwrap();
        assert_ne!(reopened.incarnation, old.incarnation);
        let actions = DependencyActions {
            basis: Some(first.basis),
            selected: Some(old.line),
            navigate: Some(old),
            ..Default::default()
        };
        assert!(
            dialog
                .commit(
                    actions,
                    Some(&reopened),
                    Some((reopened.document_id, reopened.incarnation)),
                    &EvaluationStatus::Ready
                )
                .is_none()
        );
        assert!(dialog.selected.is_none());
    }

    #[test]
    fn dependency_actions_require_revision_and_pending_preserves_selection() {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(dir.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("a = 1\nb = a + 1").unwrap();
        session.evaluation();
        let before = session.dependency_snapshot().unwrap();
        let selected = before.rows[0].id;
        let target = before.rows[1].id;
        let mut dialog = CalculationDependencyDialog::default();
        dialog.open(Some((before.document_id, before.incarnation, selected)));
        assert!(
            dialog
                .commit(
                    DependencyActions::default(),
                    None,
                    Some((before.document_id, before.incarnation)),
                    &EvaluationStatus::Pending
                )
                .is_none()
        );
        assert_eq!(dialog.test_selected(), Some(selected));
        session.replace_source("a = 2\nb = a + 1").unwrap();
        session.evaluation();
        let after = session.dependency_snapshot().unwrap();
        assert_eq!(after.rows[1].id, target);
        let actions = DependencyActions {
            basis: Some(before.basis),
            selected: Some(target),
            navigate: Some(DependencyNavigation {
                document: before.document_id,
                incarnation: before.incarnation,
                line: target,
                basis: Some(before.basis),
            }),
            ..Default::default()
        };
        assert!(
            dialog
                .commit(
                    actions,
                    Some(&after),
                    Some((after.document_id, after.incarnation)),
                    &EvaluationStatus::Ready
                )
                .is_none()
        );
        assert_eq!(dialog.test_selected(), Some(selected));
    }

    #[test]
    fn dependency_companion_tab_reveals_the_focused_source_in_both_bodies() {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(dir.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        let source = (1..=24)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        session.replace_source(source).unwrap();
        session.evaluation();
        let runtime = DependencyRuntime {
            snapshot: session.dependency_snapshot(),
            identity: session.current_id().zip(session.document_incarnation()),
            status: session.evaluation_status(),
            pad_host: None,
            pinned: false,
        };

        for presentation in [UtilityPresentation::Docked, UtilityPresentation::Popout] {
            let ctx = egui::Context::default();
            ctx.all_styles_mut(|style| {
                style.animation_time = 0.0;
                style.scroll_animation = egui::style::ScrollAnimation::none();
            });
            let mut dialog = CalculationDependencyDialog::default();
            dialog.open(None);
            let frame = |dialog: &mut CalculationDependencyDialog, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(360.0, 260.0),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let child = (presentation == UtilityPresentation::Popout).then_some(ui);
                        let actions = dialog.render_host(
                            &ctx,
                            &runtime,
                            true,
                            true,
                            child,
                            presentation,
                            None,
                        );
                        assert!(actions.selected.is_none() && actions.navigate.is_none());
                    },
                )
                .drop_without_applying_deltas();
            };
            for _ in 0..4 {
                frame(&mut dialog, Vec::new());
            }
            let target = dialog.test_last_source.unwrap();
            let (rect, clip) = dialog.test_control("row/24").unwrap();
            assert!(!clip.contains_rect(rect));
            let tab = |pressed| egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            for _ in 0..48 {
                frame(&mut dialog, vec![tab(true)]);
                frame(&mut dialog, vec![tab(false)]);
                if ctx.memory(|m| m.focused()) == Some(target) {
                    break;
                }
            }
            frame(&mut dialog, Vec::new());
            assert_eq!(
                ctx.memory(|m| m.focused()),
                Some(target),
                "{presentation:?}"
            );
            let (rect, clip) = dialog.test_control("row/24").unwrap();
            assert!(
                clip.contains_rect(rect),
                "{presentation:?}: {rect:?} vs {clip:?}"
            );
        }
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
            identity: session.current_id().zip(session.document_incarnation()),
            status: session.evaluation_status(),
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
                    None,
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
