//! Explicit local-XSD selection and validation controls.
//!
//! This view only emits user intents. File selection and XML engine work stay
//! with the caller so neither filesystem access nor COM work runs in egui's
//! frame path.

use crate::xml_toolkit_engine::{XmlDiagnostic, XmlEngineError};
use crate::xml_toolkit_session::{
    XmlParseStatus, XmlSchemaSessionState, XmlSchemaStatus, XmlSessionSnapshot, XmlTreeStatus,
};
use eframe::egui;

const MAX_SCHEMA_PATH_CHARS: usize = 512;
const MAX_SCHEMA_DIAGNOSTIC_CHARS: usize = 4096;

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
        let first_pass = ui.ctx().current_pass_index() == 0;
        let mut choose_clicked = false;
        let mut clear_clicked = false;
        let mut validate_clicked = false;

        ui.heading("XSD validation");
        ui.horizontal(|ui| {
            let choose = ui.button("Choose local XSD…");
            #[cfg(test)]
            {
                self.choose_button_rect = Some(choose.rect);
            }
            choose_clicked = choose.clicked();
            clear_clicked = ui
                .add_enabled(
                    snapshot.schema.selected_path.is_some(),
                    egui::Button::new("Clear XSD"),
                )
                .clicked();
            let can_validate = snapshot.schema.selected_path.is_some()
                && snapshot.parse == XmlParseStatus::Ready
                && matches!(&snapshot.tree, XmlTreeStatus::Ready(_));
            validate_clicked = ui
                .add_enabled(can_validate, egui::Button::new("Validate against XSD"))
                .clicked();
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

        if first_pass {
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
        match &schema.status {
            XmlSchemaStatus::NotSelected => {}
            XmlSchemaStatus::NotRun => {
                ui.weak("Validation has not been run for the current XML and XSD.");
            }
            XmlSchemaStatus::Pending(request) if request_is_current(schema, snapshot, request) => {
                ui.weak("Validating the current XML against the selected local XSD…");
            }
            XmlSchemaStatus::Valid(request) if request_is_current(schema, snapshot, request) => {
                ui.label("XML is valid against the selected XSD.");
            }
            XmlSchemaStatus::SchemaViolation {
                request,
                diagnostic,
            } if request_is_current(schema, snapshot, request) => {
                let message = schema_diagnostic_message(diagnostic);
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("XML instance violates the selected XSD: {message}"),
                );
            }
            XmlSchemaStatus::SchemaLoadParseResolveCompileError { request, error }
                if request_is_current(schema, snapshot, request) =>
            {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "The selected XSD schema graph could not be loaded, resolved, or compiled: {}",
                        schema_error_message(error)
                    ),
                );
            }
            XmlSchemaStatus::Unavailable { request, error }
                if request_is_current(schema, snapshot, request) =>
            {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "XSD validation is unavailable: {}",
                        schema_error_message(error)
                    ),
                );
            }
            XmlSchemaStatus::Pending(_)
            | XmlSchemaStatus::Valid(_)
            | XmlSchemaStatus::SchemaViolation { .. }
            | XmlSchemaStatus::SchemaLoadParseResolveCompileError { .. }
            | XmlSchemaStatus::Unavailable { .. } => {
                // A stale status is never presented as applying to the current
                // source/schema pair, even if a caller constructed a snapshot
                // that bypassed the session's normal invalidation path.
                ui.weak("Validation has not been run for the current XML and XSD.");
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

fn request_is_current(
    schema: &XmlSchemaSessionState,
    snapshot: &XmlSessionSnapshot,
    request: &crate::xml_toolkit_session::XmlSchemaValidationRequest,
) -> bool {
    snapshot.revision == Some(request.source_revision)
        && snapshot.parse == XmlParseStatus::Ready
        && matches!(&snapshot.tree, XmlTreeStatus::Ready(tree) if tree.document == request.document)
        && schema.selected_path.as_ref() == Some(&request.schema_path)
        && schema.revision == request.schema_revision
}

fn schema_diagnostic_message(diagnostic: &XmlDiagnostic) -> String {
    let location = match (diagnostic.line, diagnostic.column) {
        (Some(line), Some(column)) => format!(" at XML line {line}, column {column}"),
        (Some(line), None) => format!(" at XML line {line}"),
        (None, Some(column)) => format!(" at XML column {column}"),
        (None, None) => String::new(),
    };
    let message = bounded_message(&diagnostic.message);
    if location.is_empty() {
        message
    } else {
        format!("{location}: {message}")
    }
}

fn schema_error_message(error: &XmlEngineError) -> String {
    match error {
        XmlEngineError::SchemaLoad(diagnostic) => {
            let location = match (diagnostic.line, diagnostic.column) {
                (Some(line), Some(column)) => {
                    format!(" (schema graph line {line}, column {column})")
                }
                (Some(line), None) => format!(" (schema graph line {line})"),
                (None, Some(column)) => format!(" (schema graph column {column})"),
                (None, None) => String::new(),
            };
            let message = bounded_message(&diagnostic.message);
            if location.is_empty() {
                message
            } else {
                format!("{location}: {message}")
            }
        }
        XmlEngineError::SchemaPolicy(message) => {
            format!("XSD resolution policy: {}", bounded_message(message))
        }
        XmlEngineError::Io(message) => {
            format!("local XSD file access: {}", bounded_message(message))
        }
        XmlEngineError::SchemaBackend(message) => {
            format!("MSXML backend: {}", bounded_message(message))
        }
        XmlEngineError::UnsupportedPlatform => "this platform is unsupported".into(),
        XmlEngineError::WorkerUnavailable => "XML worker is unavailable".into(),
        other => bounded_message(&format!("{other:?}")),
    }
}

fn bounded_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    bounded_message_with_limit(&text, MAX_SCHEMA_PATH_CHARS)
}

fn bounded_message(message: &str) -> String {
    bounded_message_with_limit(message, MAX_SCHEMA_DIAGNOSTIC_CHARS)
}

fn bounded_message_with_limit(message: &str, limit: usize) -> String {
    let mut chars = message.chars();
    let prefix: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{prefix}… (display clipped)")
    } else {
        prefix
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

        let mut stale = current.clone();
        stale.revision = Some(XmlSourceRevision::for_test(2));
        assert!(!request_is_current(&stale.schema, &stale, request));
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
}
