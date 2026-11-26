//! Explicit local-XSD selection and validation controls.
//!
//! This view only emits user intents. File selection and XML engine work stay
//! with the caller so neither filesystem access nor COM work runs in egui's
//! frame path.

use crate::gui::xml_toolkit_dialog::XmlJumpTargets;
use crate::xml_toolkit_engine::{XmlDiagnostic, XmlEngineError};
use crate::xml_schema_view::{request_is_current, schema_diagnostic_message, schema_error_message, bounded_path, MAX_SCHEMA_DIAGNOSTIC_CHARS};
use crate::xml_toolkit_session::{
    XmlParseStatus, XmlSchemaSessionState, XmlSchemaStatus, XmlSessionSnapshot, XmlTreeStatus,
};
use eframe::egui;


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum XmlToolkitSchemaIntent {
    ChooseSchema,
    ClearSchema,
    Validate,
}

#[derive(Debug, Default)]
pub(crate) struct XmlToolkitSchemaViewState {
    pending_intent: Option<XmlToolkitSchemaIntent>,
    #[cfg(test)]
    choose_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    intent_commits: usize,
}

impl XmlToolkitSchemaViewState {
    /// Draws the selector/status controls. The caller consumes the committed
    /// intent after rendering and performs any picker or session operation.
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, snapshot: &XmlSessionSnapshot) {
        self.show_scoped(ui, snapshot, &mut XmlJumpTargets::new(None), true);
    }

    pub(crate) fn show_scoped(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &XmlSessionSnapshot,
        targets: &mut XmlJumpTargets<'_>,
        authorized_pass: bool,
    ) {
        let first_pass = ui.ctx().current_pass_index() == 0;
        let mut choose_clicked = false;
        let mut clear_clicked = false;
        let mut validate_clicked = false;
        let action_revision = format!("{:?}/{:?}", snapshot.revision, snapshot.schema.revision);
        let choose_identity = format!("validation/{action_revision}/choose-schema");
        let clear_identity = format!("validation/{action_revision}/clear-schema");
        let validate_identity = format!("validation/{action_revision}/validate");

        ui.heading("XSD validation");
        ui.push_id((snapshot.revision, snapshot.schema.revision), |ui| {
            ui.horizontal(|ui| {
                let choose = ui.button("Choose local XSD…");
                #[cfg(test)]
                {
                    self.choose_button_rect = Some(choose.rect);
                }
                targets.button(ui, &choose, &choose_identity, "Choose local XML schema");
                choose_clicked = choose.clicked();
                let clear = ui.add_enabled(
                    snapshot.schema.selected_path.is_some(),
                    egui::Button::new("Clear XSD"),
                );
                targets.button(ui, &clear, &clear_identity, "Clear selected XML schema");
                clear_clicked = clear.clicked();
                let can_validate = snapshot.schema.selected_path.is_some()
                    && snapshot.parse == XmlParseStatus::Ready
                    && matches!(&snapshot.tree, XmlTreeStatus::Ready(_));
                let validate =
                    ui.add_enabled(can_validate, egui::Button::new("Validate against XSD"));
                targets.button(
                    ui,
                    &validate,
                    &validate_identity,
                    "Validate XML against schema",
                );
                validate_clicked = validate.clicked();
            });
        });

        if let Some(path) = &snapshot.schema.selected_path {
            ui.label(format!("Selected XSD: {}", bounded_path(path.as_path())));
        } else {
            ui.weak("No local XSD selected.");
        }
        ui.weak(
            "Resolution stays under the selected XSD folder and accepts relative xs:include and xs:import paths. Imports need an explicit schemaLocation; imports inside included files are unsupported. xsi:schemaLocation is ignored, and network or absolute references are never fetched.",
        );
        self.show_status(ui, &snapshot.schema, snapshot);

        if first_pass && authorized_pass {
            let intent = if choose_clicked {
                Some(XmlToolkitSchemaIntent::ChooseSchema)
            } else if clear_clicked {
                Some(XmlToolkitSchemaIntent::ClearSchema)
            } else if validate_clicked {
                Some(XmlToolkitSchemaIntent::Validate)
            } else {
                None
            };
            if let Some(intent) = intent {
                self.pending_intent = Some(intent);
                #[cfg(test)]
                {
                    self.intent_commits += 1;
                }
            }
        }
    }

    /// Takes a committed action once, outside the rendering pass.
    pub(crate) fn take_intent(&mut self) -> Option<XmlToolkitSchemaIntent> {
        self.pending_intent.take()
    }

    fn show_status(
        &self,
        ui: &mut egui::Ui,
        schema: &XmlSchemaSessionState,
        snapshot: &XmlSessionSnapshot,
    ) {
        let _ = schema;
        if let Some((message, tone)) = crate::xml_schema_view::status_message(snapshot) {
            match tone {
                crate::xml_schema_view::SchemaStatusTone::Weak => { ui.weak(message); }
                crate::xml_schema_view::SchemaStatusTone::Normal => { ui.label(message); }
                crate::xml_schema_view::SchemaStatusTone::Error => { ui.colored_label(ui.visuals().error_fg_color, message); }
            }
        }
    }

    #[cfg(test)]
    fn choose_button_rect(&self) -> Option<egui::Rect> {
        self.choose_button_rect
    }

    #[cfg(test)]
    fn intent_commits(&self) -> usize {
        self.intent_commits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_toolkit_engine::{XmlDocumentId, XmlTreeProjection};
    use crate::xml_toolkit_session::{
        XmlSchemaPath, XmlSchemaRequestId, XmlSchemaRevision, XmlSchemaValidationRequest,
        XmlSourceRevision,
    };
    use std::sync::Arc;

    fn snapshot() -> XmlSessionSnapshot {
        let document = XmlDocumentId::for_test(1);
        let projection = Arc::new(XmlTreeProjection::from_nodes(
            document,
            crate::xml_toolkit_engine::XmlNodeId::for_test(document, 1),
            Vec::new(),
            None,
        ));
        let revision = XmlSourceRevision::for_test(1);
        let schema_path = XmlSchemaPath::new("schema.xsd".into());
        let request = XmlSchemaValidationRequest {
            source_revision: revision,
            document,
            schema_path: schema_path.clone(),
            schema_revision: XmlSchemaRevision::for_test(1),
            request: XmlSchemaRequestId::from_counter(1),
        };
        XmlSessionSnapshot {
            revision: Some(revision),
            parse: XmlParseStatus::Ready,
            output: crate::xml_toolkit_session::XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Ready(projection),
            detail: crate::xml_toolkit_session::XmlDetailStatus::Empty,
            xpath: crate::xml_toolkit_session::XmlXPathStatus::NotRun,
            schema: XmlSchemaSessionState {
                selected_path: Some(schema_path),
                revision: request.schema_revision,
                status: XmlSchemaStatus::Valid(request),
            },
        }
    }

    #[test]
    fn m7e_schema_view_discards_stale_results_and_clips_long_diagnostics() {
        let current = snapshot();
        let XmlSchemaStatus::Valid(request) = &current.schema.status else {
            panic!("fixture contains a valid status");
        };
        assert!(request_is_current(&current.schema, &current, request));
        assert_eq!(
            crate::xml_schema_view::status_message(&current).unwrap().1,
            crate::xml_schema_view::SchemaStatusTone::Normal
        );

        let mut stale = current.clone();
        stale.revision = Some(XmlSourceRevision::for_test(2));
        assert!(!request_is_current(&stale.schema, &stale, request));
        assert_eq!(
            crate::xml_schema_view::status_message(&stale).unwrap().1,
            crate::xml_schema_view::SchemaStatusTone::Weak
        );
        assert!(
            schema_diagnostic_message(&XmlDiagnostic {
                line: None,
                column: None,
                message: "x".repeat(MAX_SCHEMA_DIAGNOSTIC_CHARS + 1),
            })
            .contains("display clipped")
        );
        assert!(
            schema_error_message(&XmlEngineError::SchemaPolicy("denied".into()))
                .contains("XSD resolution policy")
        );
    }

    #[test]
    fn m7e_schema_picker_intent_is_committed_once_before_frame_discard() {
        let snapshot = snapshot();
        let context = egui::Context::default();
        let mut view = XmlToolkitSchemaViewState::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 300.0));
        let first = context.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    view.show(ui, &snapshot);
                });
            },
        );
        first.drop_without_applying_deltas();
        let choose = view
            .choose_button_rect()
            .expect("choose button was rendered");
        let pointer = choose.center();

        let pressed = context.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                events: vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::PointerButton {
                        pos: pointer,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    view.show(ui, &snapshot);
                });
            },
        );
        pressed.drop_without_applying_deltas();
        let released = context.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                events: vec![egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                }],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    view.show(ui, &snapshot);
                    if ui.ctx().current_pass_index() == 0 {
                        ui.ctx().request_discard("test schema picker action");
                    }
                });
            },
        );
        released.drop_without_applying_deltas();

        assert_eq!(view.intent_commits(), 1);
        assert_eq!(
            view.take_intent(),
            Some(XmlToolkitSchemaIntent::ChooseSchema)
        );
        assert_eq!(view.take_intent(), None);
    }

    #[test]
    fn jump_mode_tool_viewports_xml_schema_actions_follow_schema_revision() {
        use crate::gui::jump_mode::ui::{NavigationDialogKind, NavigationOwner};
        use crate::gui::jump_mode::{SurfaceId, TargetGeneration};

        let context = egui::Context::default();
        let mut jump = crate::gui::jump_mode::ui::JumpModeUi::new(true);
        let mut view = XmlToolkitSchemaViewState::default();
        let mut current = snapshot();
        let visibility_changed =
            jump.observe_dialog_visibility([(NavigationDialogKind::XmlToolkit, true)]);
        let instance = jump
            .dialog_instance(NavigationDialogKind::XmlToolkit)
            .expect("the XML Toolkit window has a per-open instance");
        let labels = [
            "Choose local XML schema",
            "Clear selected XML schema",
            "Validate XML against schema",
        ];

        let warm = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 700.0),
                )),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                egui::Window::new("XML Toolkit")
                    .id(egui::Id::new("xml_toolkit_embedded_window"))
                    .show(ctx, |_| {});
            },
        );
        warm.drop_without_applying_deltas();

        let mut render = |snapshot: &XmlSessionSnapshot, owner_transition: bool| {
            jump.begin_frame();
            let owner = NavigationOwner::Dialog {
                kind: NavigationDialogKind::XmlToolkit,
                instance,
            };
            let mut frame_owner = NavigationOwner::Blocked;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 700.0),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    jump.handle_root_input(ctx, false, true);
                    frame_owner = jump.update_navigation_owner(ctx, owner, owner_transition);
                    jump.begin_ui_pass();
                    let scope = (frame_owner == owner)
                        .then(|| {
                            jump.scope_for_navigation_owner(
                                SurfaceId::dialog("xml-toolkit", instance),
                                TargetGeneration(instance),
                                "xml-toolkit/form",
                                egui::ViewportId::ROOT,
                                owner,
                                true,
                            )
                        })
                        .flatten();
                    egui::Window::new("XML Toolkit")
                        .id(egui::Id::new("xml_toolkit_embedded_window"))
                        .default_size(egui::vec2(1000.0, 420.0))
                        .show(ctx, |ui| {
                            view.show_scoped(
                                ui,
                                snapshot,
                                &mut XmlJumpTargets::new(scope.as_ref()),
                                true,
                            );
                        });
                },
            );
            output.drop_without_applying_deltas();
            jump.finish_ui_pass();
            jump.prepare_pass_for_navigation_owner(&context, frame_owner, true, true);
            jump.complete_frame_for_navigation_owner(&context, frame_owner, true, true);
            let targets = (frame_owner == owner).then(|| {
                labels.map(|label| {
                    jump.retained_registry()
                        .targets()
                        .iter()
                        .find(|target| target.metadata().label() == label)
                        .unwrap_or_else(|| {
                            panic!(
                                "schema action {label:?} missing; collected labels: {:?}",
                                jump.retained_registry()
                                    .metadata()
                                    .map(|metadata| metadata.label())
                                    .collect::<Vec<_>>()
                            )
                        })
                        .clone()
                })
            });
            (frame_owner, targets)
        };

        let (opening_owner, _) = render(&current, visibility_changed);
        assert_eq!(
            opening_owner,
            NavigationOwner::Blocked,
            "the opening owner-transition frame is not collected"
        );
        let (owner, first_targets) = render(&current, false);
        assert_eq!(
            owner,
            NavigationOwner::Dialog {
                kind: NavigationDialogKind::XmlToolkit,
                instance,
            }
        );
        let first_targets = first_targets.expect("the current owner collected schema actions");
        current.schema.revision = XmlSchemaRevision::for_test(2);
        let (second_owner, second_targets) = render(&current, false);
        assert_eq!(second_owner, owner);
        let second_targets = second_targets.expect("all schema actions remain enabled and visible");

        assert!(
            first_targets
                .iter()
                .zip(second_targets.iter())
                .all(|(first, second)| first.id() != second.id())
        );
        assert!(
            first_targets
                .iter()
                .zip(second_targets.iter())
                .all(|(first, second)| first.response_id() != second.response_id()),
            "the actual native action responses are scoped to source/schema revision"
        );
        assert!(
            first_targets
                .iter()
                .chain(second_targets.iter())
                .all(|target| match target.id().semantic() {
                    crate::gui::jump_mode::SemanticTargetId::Named(identity) => {
                        !identity.contains("schema.xsd")
                    }
                    _ => true,
                }),
            "schema paths never enter semantic target identity"
        );
    }
}
