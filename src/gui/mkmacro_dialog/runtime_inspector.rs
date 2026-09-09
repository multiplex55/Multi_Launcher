use super::MkMacroDialog;
use crate::mkmacro::{
    CompletedStepOutcome, DebugSnapshotReason, MacroStepKey, MkMacroDocument, MkValue,
    RuntimePauseReason, RuntimeRunMode, RuntimeSnapshot, RuntimeState, is_builtin,
};
use eframe::egui;

const STRING_TABLE_CHAR_LIMIT: usize = 80;
pub const INSPECTOR_BODY_HEIGHT: f32 = 180.0;
const NULL_VALUE_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 180, 70);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDisplayStyle {
    Normal,
    NullMutedWarning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedValue {
    pub table_text: String,
    pub hover_text: String,
    pub type_name: &'static str,
    pub style: ValueDisplayStyle,
}

impl FormattedValue {
    pub fn is_null(&self) -> bool {
        self.style == ValueDisplayStyle::NullMutedWarning
    }
}

/// Format one runtime value for both the compact table and its complete hover.
/// The source string is truncated before escaping, so truncation always occurs
/// on a Unicode scalar boundary and never splits an escaped sequence.
pub fn format_value(value: &MkValue) -> FormattedValue {
    match value {
        MkValue::String(value) => {
            let hover_text = quoted_string(value);
            let table_text = if value.chars().count() > STRING_TABLE_CHAR_LIMIT {
                let prefix: String = value.chars().take(STRING_TABLE_CHAR_LIMIT).collect();
                let mut table = quoted_string(&prefix);
                table.pop();
                table.push('…');
                table.push('"');
                table
            } else {
                hover_text.clone()
            };
            FormattedValue {
                table_text,
                hover_text,
                type_name: "String",
                style: ValueDisplayStyle::Normal,
            }
        }
        MkValue::Number(value) => {
            let text = format_number(*value);
            FormattedValue {
                table_text: text.clone(),
                hover_text: text,
                type_name: "Number",
                style: ValueDisplayStyle::Normal,
            }
        }
        MkValue::Boolean(value) => FormattedValue {
            table_text: value.to_string(),
            hover_text: value.to_string(),
            type_name: "Boolean",
            style: ValueDisplayStyle::Normal,
        },
        MkValue::Point(point) => {
            let text = format!("({}, {})", point.x, point.y);
            FormattedValue {
                table_text: text.clone(),
                hover_text: text,
                type_name: "Point",
                style: ValueDisplayStyle::Normal,
            }
        }
        MkValue::Null => FormattedValue {
            table_text: "null".into(),
            hover_text: "null".into(),
            type_name: "Null",
            style: ValueDisplayStyle::NullMutedWarning,
        },
    }
}

fn quoted_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\0' => escaped.push_str("\\0"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\x08' => escaped.push_str("\\b"),
            '\x0c' => escaped.push_str("\\f"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{{{:04x}}}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn format_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "∞".into()
    } else if value == f64::NEG_INFINITY {
        "-∞".into()
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableGroupKind {
    User,
    BuiltIns,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableEntry {
    pub name: String,
    pub value: FormattedValue,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableGroups {
    pub user: Vec<VariableEntry>,
    pub built_ins: Vec<VariableEntry>,
    pub internal: Vec<VariableEntry>,
}

impl VariableGroups {
    pub fn from_variables(variables: &crate::mkmacro::RuntimeVariables) -> Self {
        let mut groups = Self::default();
        for (name, value) in variables {
            let entry = VariableEntry {
                name: name.clone(),
                value: format_value(value),
            };
            if name.starts_with("__") {
                groups.internal.push(entry);
            } else if is_builtin(name) {
                groups.built_ins.push(entry);
            } else {
                groups.user.push(entry);
            }
        }
        groups
    }

    pub fn group(&self, kind: VariableGroupKind) -> &[VariableEntry] {
        match kind {
            VariableGroupKind::User => &self.user,
            VariableGroupKind::BuiltIns => &self.built_ins,
            VariableGroupKind::Internal => &self.internal,
        }
    }

    pub fn visible_internal(&self, show_internal: bool) -> &[VariableEntry] {
        if show_internal { &self.internal } else { &[] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepPresentation {
    pub macro_id: u64,
    pub step_id: u64,
    pub row_number: Option<usize>,
    pub action_name: String,
}

impl StepPresentation {
    pub fn label(&self) -> String {
        match self.row_number {
            Some(row) => format!("#{row} {}", self.action_name),
            None => format!("step {} (definition unavailable)", self.step_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastOutcomePresentation {
    Success {
        detail: Option<String>,
    },
    Failure {
        message: String,
        context: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallStackPresentation {
    pub frame_id: u64,
    pub macro_name: String,
    pub caller_step: Option<String>,
    pub current_step: Option<String>,
    pub depth: usize,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInspectorViewModel {
    pub title: String,
    pub macro_name: String,
    pub status: String,
    pub current_step: Option<StepPresentation>,
    pub last_step: Option<StepPresentation>,
    pub last_step_macro_name: Option<String>,
    pub call_stack: Vec<CallStackPresentation>,
    pub breakpoint_pause: bool,
    pub variable_label: String,
    pub snapshot_reason: String,
    pub outcome: Option<LastOutcomePresentation>,
    pub run_failure: Option<LastOutcomePresentation>,
    pub variables: VariableGroups,
}

impl RuntimeInspectorViewModel {
    pub fn from_snapshot(snapshot: &RuntimeSnapshot, document: &MkMacroDocument) -> Self {
        let current = matches!(
            (snapshot.run_mode, snapshot.state),
            (
                RuntimeRunMode::Debug,
                RuntimeState::Running | RuntimeState::Paused | RuntimeState::Stopping
            )
        );
        Self::from_snapshot_with_retention(snapshot, document, current)
    }

    pub fn from_snapshot_with_retention(
        snapshot: &RuntimeSnapshot,
        document: &MkMacroDocument,
        current_debug_run: bool,
    ) -> Self {
        let reason = snapshot_reason(snapshot);
        let variables = snapshot_variables(snapshot);
        let breakpoint_pause = matches!(
            snapshot.pause_reason,
            Some(RuntimePauseReason::Breakpoint { .. })
        );
        let current_key = match snapshot.pause_reason {
            Some(RuntimePauseReason::Breakpoint { step_id, frame }) => {
                Some(MacroStepKey::new(frame.macro_id, step_id))
            }
            Some(RuntimePauseReason::User) => None,
            None if matches!(
                snapshot.state,
                RuntimeState::Running | RuntimeState::Stopping
            ) =>
            {
                snapshot
                    .active_macro_id()
                    .zip(snapshot.active_step_id())
                    .map(|(macro_id, step_id)| MacroStepKey::new(macro_id, step_id))
            }
            None => None,
        };
        let current_step = current_key.map(|key| resolve_step(document, key));
        let last_step = snapshot
            .last_completed
            .as_ref()
            .map(|completed| resolve_step(document, completed.key));
        let last_step_macro_name = snapshot
            .last_completed
            .as_ref()
            .map(|completed| completed.macro_name.to_string());
        let outcome = last_outcome(snapshot);
        let run_failure = snapshot.latest_failure.as_ref().filter(|failure| {
            snapshot.state == RuntimeState::Failed && !snapshot.last_completed.as_ref().is_some_and(|completed|
                matches!(&completed.outcome, CompletedStepOutcome::Failure(recorded) if recorded == *failure))
        }).map(failure_presentation);
        let call_stack = snapshot
            .call_stack
            .iter()
            .enumerate()
            .map(|(index, frame)| CallStackPresentation {
                frame_id: frame.context.frame_id,
                macro_name: frame.macro_name.to_string(),
                depth: frame.context.depth,
                active: index + 1 == snapshot.call_stack.len(),
                caller_step: frame.context.caller_step_id.and_then(|step_id| {
                    index.checked_sub(1).map(|parent| {
                        resolve_step(
                            document,
                            MacroStepKey::new(
                                snapshot.call_stack[parent].context.macro_id,
                                step_id,
                            ),
                        )
                        .label()
                    })
                }),
                current_step: frame.active_step_id.map(|step_id| {
                    resolve_step(document, MacroStepKey::new(frame.context.macro_id, step_id))
                        .label()
                }),
            })
            .collect();
        Self {
            title: match (snapshot.run_mode, current_debug_run) {
                (RuntimeRunMode::Debug, true) => "Runtime Inspector — Current Debug Run".into(),
                (RuntimeRunMode::Debug, false) => "Runtime Inspector — Last Debug Run".into(),
                (RuntimeRunMode::Normal, _) => "Runtime Inspector — Normal Run".into(),
            },
            macro_name: snapshot
                .call_stack
                .last()
                .map(|frame| frame.macro_name.to_string())
                .or_else(|| {
                    snapshot
                        .debug_snapshot
                        .as_ref()
                        .map(|debug| debug.macro_name.to_string())
                })
                .or_else(|| snapshot.root_macro_name.as_ref().map(ToString::to_string))
                .unwrap_or_else(|| resolve_macro_name(document, snapshot.macro_id)),
            status: status_text(snapshot),
            current_step,
            last_step,
            last_step_macro_name,
            call_stack,
            breakpoint_pause,
            variable_label: variable_label(snapshot, reason),
            snapshot_reason: reason
                .map(debug_snapshot_reason_description)
                .unwrap_or("No debug snapshot reason published.")
                .into(),
            outcome,
            run_failure,
            variables: VariableGroups::from_variables(variables),
        }
    }
}

fn resolve_macro_name(document: &MkMacroDocument, macro_id: Option<u64>) -> String {
    match macro_id {
        Some(id) => document
            .macros
            .iter()
            .find(|macro_| macro_.id == id)
            .map(|macro_| macro_.name.clone())
            .unwrap_or_else(|| format!("Removed macro #{id}")),
        None => "Unknown macro".into(),
    }
}

fn resolve_step(document: &MkMacroDocument, key: MacroStepKey) -> StepPresentation {
    let MacroStepKey { macro_id, step_id } = key;
    let step = document
        .macros
        .iter()
        .find(|macro_| macro_.id == macro_id)
        .and_then(|macro_| {
            macro_
                .steps
                .iter()
                .enumerate()
                .find(|(_, step)| step.id == step_id)
        });
    match step {
        Some((index, step)) => StepPresentation {
            macro_id,
            step_id,
            row_number: Some(index + 1),
            action_name: super::action_catalog::action_name(&step.action).into(),
        },
        None => StepPresentation {
            macro_id,
            step_id,
            row_number: None,
            action_name: "Step definition unavailable".into(),
        },
    }
}

fn snapshot_variables(snapshot: &RuntimeSnapshot) -> &crate::mkmacro::RuntimeVariables {
    snapshot
        .debug_snapshot
        .as_ref()
        .map(|debug| debug.variables.as_ref())
        .unwrap_or_else(|| snapshot.debug_variables.as_ref())
}

fn snapshot_reason(snapshot: &RuntimeSnapshot) -> Option<DebugSnapshotReason> {
    snapshot
        .debug_snapshot
        .as_ref()
        .map(|debug| debug.reason)
        .or(snapshot.debug_snapshot_reason)
}

pub fn debug_snapshot_reason_description(reason: DebugSnapshotReason) -> &'static str {
    match reason {
        DebugSnapshotReason::RunStarted => "Data captured at debug run start.",
        DebugSnapshotReason::FrameRestored => "Restored the caller’s last safe execution boundary.",
        DebugSnapshotReason::Breakpoint => "Data captured before the breakpoint step.",
        DebugSnapshotReason::StepBoundary => "Data captured at the last safe step boundary.",
        DebugSnapshotReason::RunFinished => "Data captured at successful completion.",
        DebugSnapshotReason::RunCancelled => "Data captured at cancellation.",
        DebugSnapshotReason::RunFailed => "Data captured at failure.",
    }
}

pub fn status_text(snapshot: &RuntimeSnapshot) -> String {
    match snapshot.state {
        RuntimeState::Idle => "Idle".into(),
        RuntimeState::Running => match snapshot_reason(snapshot) {
            Some(DebugSnapshotReason::RunStarted) => "Running — debug run started".into(),
            _ => "Running".into(),
        },
        RuntimeState::Paused => match snapshot.pause_reason {
            Some(RuntimePauseReason::Breakpoint { .. }) => "Paused at breakpoint".into(),
            Some(RuntimePauseReason::User) => "Paused manually".into(),
            None => "Paused".into(),
        },
        RuntimeState::Stopping => "Stopping".into(),
        RuntimeState::Completed => "Completed".into(),
        RuntimeState::Stopped => "Stopped".into(),
        RuntimeState::Failed => "Failed".into(),
    }
}

fn variable_label(snapshot: &RuntimeSnapshot, reason: Option<DebugSnapshotReason>) -> String {
    if snapshot.pause_reason == Some(RuntimePauseReason::User) {
        return "Last safe execution boundary".into();
    }
    match reason {
        Some(DebugSnapshotReason::RunStarted) => "Variables at debug run start".into(),
        Some(DebugSnapshotReason::FrameRestored) => {
            "Caller variables at its last safe boundary".into()
        }
        Some(DebugSnapshotReason::Breakpoint) => "Variables before breakpoint step".into(),
        Some(DebugSnapshotReason::StepBoundary) => "Last safe execution boundary".into(),
        Some(DebugSnapshotReason::RunFinished) => "Variables at successful completion".into(),
        Some(DebugSnapshotReason::RunCancelled) => "Variables at cancellation".into(),
        Some(DebugSnapshotReason::RunFailed) => "Variables at failure".into(),
        None => "Debug variables".into(),
    }
}

fn failure_presentation(failure: &crate::mkmacro::ExecutionDiagnostic) -> LastOutcomePresentation {
    LastOutcomePresentation::Failure {
        message: failure.message.clone(),
        context: failure
            .context
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

fn last_outcome(snapshot: &RuntimeSnapshot) -> Option<LastOutcomePresentation> {
    match &snapshot.last_completed.as_ref()?.outcome {
        CompletedStepOutcome::Failure(failure) => Some(failure_presentation(failure)),
        CompletedStepOutcome::Success(Some(outcome)) => Some(LastOutcomePresentation::Success {
            detail: outcome.detail().map(str::to_owned),
        }),
        CompletedStepOutcome::Success(None) => None,
    }
}

pub fn inspector_body_height() -> f32 {
    INSPECTOR_BODY_HEIGHT.clamp(150.0, 220.0)
}

fn render_value(ui: &mut egui::Ui, value: &FormattedValue) {
    let text = egui::RichText::new(&value.table_text);
    let text = if value.is_null() {
        text.color(NULL_VALUE_COLOR)
    } else {
        text
    };
    ui.label(text).on_hover_text(&value.hover_text);
}

fn render_variable_table(ui: &mut egui::Ui, id: &'static str, entries: &[VariableEntry]) {
    egui::Grid::new(id)
        .striped(true)
        .num_columns(3)
        .show(ui, |ui| {
            ui.strong("Name");
            ui.strong("Type");
            ui.strong("Value");
            ui.end_row();
            for entry in entries {
                ui.label(&entry.name);
                ui.label(entry.value.type_name);
                render_value(ui, &entry.value);
                ui.end_row();
            }
        });
}

fn apply_header_intent(open: &mut bool, activated: bool) {
    if activated {
        *open = !*open;
    }
}

fn render_variables(
    ui: &mut egui::Ui,
    groups: &VariableGroups,
    show_internal: bool,
    builtins_open: &mut bool,
) {
    egui::ScrollArea::vertical()
        .id_source("runtime_inspector_variables")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut has_visible = false;
            if !groups.user.is_empty() {
                has_visible = true;
                render_variable_table(ui, "runtime_inspector_user_variables", &groups.user);
            }
            if !groups.built_ins.is_empty() {
                has_visible = true;
                let response =
                    egui::CollapsingHeader::new(format!("Built-ins ({})", groups.built_ins.len()))
                        .open(Some(*builtins_open))
                        .show(ui, |ui| {
                            render_variable_table(
                                ui,
                                "runtime_inspector_builtin_variables",
                                &groups.built_ins,
                            );
                        });
                // `openness` is only the animation progress. The header response is
                // limited to the Built-ins header, so controls in its body cannot
                // change the persisted expansion intent.
                apply_header_intent(builtins_open, response.header_response.clicked());
            }
            if show_internal && !groups.internal.is_empty() {
                has_visible = true;
                render_variable_table(ui, "runtime_inspector_internal_variables", &groups.internal);
            }
            if !has_visible {
                ui.label("No visible runtime variables.");
            }
        });
}

fn render_outcome(ui: &mut egui::Ui, outcome: &LastOutcomePresentation) {
    match outcome {
        LastOutcomePresentation::Success { detail } => {
            ui.colored_label(egui::Color32::GREEN, detail.as_deref().unwrap_or("Success"));
        }
        LastOutcomePresentation::Failure { message, context } => {
            ui.colored_label(egui::Color32::RED, "Failed");
            ui.label(message);
            for (key, value) in context {
                ui.label(format!("{key}: {value}"));
            }
        }
    }
}

fn render_body(
    ui: &mut egui::Ui,
    view: &RuntimeInspectorViewModel,
    show_internal: &mut bool,
    builtins_open: &mut bool,
) {
    egui::ScrollArea::vertical()
        .id_source("runtime_inspector_body")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Macro:");
                ui.label(&view.macro_name);
                ui.separator();
                ui.strong(format!("Status: {}", view.status));
            });
            if view.call_stack.len() > 1 {
                ui.strong("Call Stack");
                for frame in &view.call_stack {
                    ui.horizontal(|ui| {
                        ui.add_space(frame.depth.saturating_sub(1) as f32 * 12.0);
                        let label = if frame.active {
                            format!("▶ {}", frame.macro_name)
                        } else {
                            frame.macro_name.clone()
                        };
                        let response = if frame.active {
                            ui.strong(label)
                        } else {
                            ui.label(label)
                        };
                        response.on_hover_ui(|ui| {
                            if let Some(step) = &frame.caller_step {
                                ui.label(format!("Called from {step}"));
                            }
                            if let Some(step) = &frame.current_step {
                                ui.label(format!("Current step: {step}"));
                            }
                        });
                    });
                }
            }
            if let Some(step) = &view.current_step {
                let color = if view.breakpoint_pause {
                    egui::Color32::from_rgb(255, 152, 0)
                } else {
                    egui::Color32::YELLOW
                };
                ui.colored_label(color, format!("Current Step: {}", step.label()));
            }
            if view.breakpoint_pause {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 152, 0),
                    "The step has not executed yet.",
                );
            }
            if let Some(step) = &view.last_step {
                ui.label(format!(
                    "Last Step: {} · {}",
                    view.last_step_macro_name
                        .as_deref()
                        .unwrap_or("Unknown macro"),
                    step.label()
                ));
            }
            if let Some(outcome) = &view.outcome {
                render_outcome(ui, outcome);
            }
            if let Some(failure) = &view.run_failure {
                ui.strong("Run failure");
                render_outcome(ui, failure);
            }
            ui.small(&view.snapshot_reason);
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(&view.variable_label);
                if !view.variables.internal.is_empty() {
                    ui.checkbox(show_internal, "Show internal variables");
                }
            });
            // This nested area is intentionally separate from the body/details
            // scroll area: large variable maps cannot change the inspector's
            // bounded height or consume the step-table viewport.
            render_variables(ui, &view.variables, *show_internal, builtins_open);
        });
}

pub(super) fn show(ui: &mut egui::Ui, dialog: &mut MkMacroDialog) {
    let Some(snapshot) = dialog.runtime_inspector_snapshot.clone() else {
        let response = egui::CollapsingHeader::new("Runtime Inspector — No Debug Data")
            .open(Some(dialog.runtime_inspector_open))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), inspector_body_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| ui.label("Run Debug to capture read-only runtime variables."),
                );
            });
        apply_header_intent(
            &mut dialog.runtime_inspector_open,
            response.header_response.clicked(),
        );
        return;
    };
    let view = RuntimeInspectorViewModel::from_snapshot_with_retention(
        &snapshot,
        &dialog.draft,
        dialog.runtime_inspector_is_current_debug_run,
    );
    let response = egui::CollapsingHeader::new(view.title.clone())
        .open(Some(dialog.runtime_inspector_open))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), inspector_body_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_body(
                        ui,
                        &view,
                        &mut dialog.runtime_inspector_show_internal,
                        &mut dialog.runtime_inspector_builtins_open,
                    )
                },
            );
        });
    apply_header_intent(
        &mut dialog.runtime_inspector_open,
        response.header_response.clicked(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        CompletedStep, ExecutionDiagnostic, ExecutionFrameContext, MkAction, MkDelayPayload,
        MkMacro, MkPoint, MkStep, RuntimeVariables, SCHEMA_VERSION, StepOutcome,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn document() -> MkMacroDocument {
        MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            settings: Default::default(),
            folders: vec![],
            macros: vec![MkMacro {
                signature: Default::default(),
                id: 7,
                name: "Demo".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                steps: vec![
                    MkStep {
                        metadata: Default::default(),
                        id: 101,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: Default::default(),
                        action: MkAction::Delay(MkDelayPayload::default()),
                    },
                    MkStep {
                        metadata: Default::default(),
                        id: 202,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: Default::default(),
                        action: MkAction::Delay(MkDelayPayload::default()),
                    },
                ],
            }],
        }
    }

    fn snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
            run_mode: RuntimeRunMode::Debug,
            run_id: 9,
            macro_id: Some(7),
            call_stack: Arc::new(vec![crate::mkmacro::ExecutionFrameSnapshot {
                context: ExecutionFrameContext::root(7),
                macro_name: Arc::from("Demo"),
                active_step_id: None,
            }]),
            revision: 3,
            ..RuntimeSnapshot::default()
        }
    }

    #[test]
    fn formats_all_value_variants_and_number_policies() {
        let cases = [
            (
                MkValue::String("hello".into()),
                "\"hello\"",
                "String",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::String("quote\" slash\\ newline\n".into()),
                "\"quote\\\" slash\\\\ newline\\n\"",
                "String",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(12.0),
                "12",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(-12.0),
                "-12",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(12.5),
                "12.5",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(-12.5),
                "-12.5",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(1_000_000_000_000.0),
                "1000000000000",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(-0.0),
                "-0",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(f64::NAN),
                "NaN",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(f64::INFINITY),
                "∞",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Number(f64::NEG_INFINITY),
                "-∞",
                "Number",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Boolean(true),
                "true",
                "Boolean",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Boolean(false),
                "false",
                "Boolean",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Point(MkPoint { x: 2, y: 4 }),
                "(2, 4)",
                "Point",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Point(MkPoint { x: -2, y: -4 }),
                "(-2, -4)",
                "Point",
                ValueDisplayStyle::Normal,
            ),
            (
                MkValue::Null,
                "null",
                "Null",
                ValueDisplayStyle::NullMutedWarning,
            ),
        ];
        for (value, expected_text, expected_type, expected_style) in cases {
            let formatted = format_value(&value);
            assert_eq!(formatted.table_text, expected_text, "value={value:?}");
            assert_eq!(formatted.hover_text, expected_text, "value={value:?}");
            assert_eq!(formatted.type_name, expected_type, "value={value:?}");
            assert_eq!(formatted.style, expected_style, "value={value:?}");
        }
    }

    #[test]
    fn escaping_and_unicode_truncation_keep_full_hover_text() {
        let source = String::from("quote\" slash\\ newline\n tab\t control\u{0001}");
        let formatted = format_value(&MkValue::String(source.clone()));
        assert!(formatted.table_text.contains("\\\""));
        assert!(formatted.table_text.contains("\\\\"));
        assert!(formatted.table_text.contains("\\n"));
        assert!(formatted.table_text.contains("\\t"));
        assert!(formatted.table_text.contains("\\u{0001}"));
        assert_eq!(formatted.hover_text, quoted_string(&source));

        let long_source = "界".repeat(100);
        let formatted = format_value(&MkValue::String(long_source.clone()));
        assert!(formatted.table_text.ends_with("…\""));
        assert_eq!(formatted.hover_text, quoted_string(&long_source));
        assert!(formatted.hover_text.chars().count() > formatted.table_text.chars().count());
        assert!(
            formatted
                .table_text
                .is_char_boundary(formatted.table_text.len())
        );
    }

    #[test]
    fn variable_groups_use_precedence_and_btree_order() {
        let variables = BTreeMap::from([
            ("z_user".into(), MkValue::Null),
            ("a_user".into(), MkValue::Null),
            ("mouse.y".into(), MkValue::Null),
            ("mouse.x".into(), MkValue::Null),
            ("macro.id".into(), MkValue::Number(7.0)),
            ("step.id".into(), MkValue::Number(101.0)),
            ("__image.foo".into(), MkValue::Null),
            ("__image_found.foo".into(), MkValue::Null),
            ("__mouse.x".into(), MkValue::Null),
            ("__secret".into(), MkValue::String("internal".into())),
        ]);
        let groups = VariableGroups::from_variables(&variables);
        assert_eq!(
            groups
                .user
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>(),
            ["a_user", "z_user"]
        );
        assert_eq!(
            groups
                .built_ins
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>(),
            ["macro.id", "mouse.x", "mouse.y", "step.id"]
        );
        assert_eq!(
            groups
                .internal
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>(),
            ["__image.foo", "__image_found.foo", "__mouse.x", "__secret"]
        );
        for name in [
            "a_user",
            "z_user",
            "macro.id",
            "mouse.x",
            "mouse.y",
            "step.id",
            "__image.foo",
            "__image_found.foo",
            "__mouse.x",
            "__secret",
        ] {
            let occurrences = [&groups.user, &groups.built_ins, &groups.internal]
                .into_iter()
                .flat_map(|group| group.iter())
                .filter(|entry| entry.name == name)
                .count();
            assert_eq!(occurrences, 1, "classification for {name}");
        }
        assert!(
            groups
                .user
                .iter()
                .all(|entry| !entry.name.starts_with("__"))
        );
        assert!(
            groups
                .internal
                .iter()
                .all(|entry| entry.name.starts_with("__"))
        );
        assert_eq!(groups.group(VariableGroupKind::User), &groups.user);
        assert!(groups.visible_internal(false).is_empty());
        assert_eq!(groups.visible_internal(true).len(), 4);
    }

    #[test]
    fn status_titles_pause_semantics_and_reason_labels_are_unambiguous() {
        let mut running = snapshot();
        running.state = RuntimeState::Running;
        running.debug_snapshot_reason = Some(DebugSnapshotReason::RunStarted);
        let running_view = RuntimeInspectorViewModel::from_snapshot(&running, &document());
        assert_eq!(running_view.status, "Running — debug run started");
        assert_eq!(running_view.title, "Runtime Inspector — Current Debug Run");

        let mut normal = snapshot();
        normal.run_mode = RuntimeRunMode::Normal;
        let normal_view = RuntimeInspectorViewModel::from_snapshot(&normal, &document());
        assert_eq!(normal_view.title, "Runtime Inspector — Normal Run");

        let mut breakpoint = running.clone();
        breakpoint.state = RuntimeState::Paused;
        breakpoint.pause_reason = Some(RuntimePauseReason::Breakpoint {
            step_id: 202,
            frame: crate::mkmacro::ExecutionFrameContext::root(7),
        });
        breakpoint.step_id = Some(202);
        breakpoint.debug_snapshot_reason = Some(DebugSnapshotReason::Breakpoint);
        let view = RuntimeInspectorViewModel::from_snapshot(&breakpoint, &document());
        assert_eq!(view.status, "Paused at breakpoint");
        assert!(view.breakpoint_pause);
        assert_eq!(view.current_step.as_ref().unwrap().label(), "#2 Delay");
        assert_eq!(view.variable_label, "Variables before breakpoint step");

        let mut manual = breakpoint.clone();
        manual.pause_reason = Some(RuntimePauseReason::User);
        let view = RuntimeInspectorViewModel::from_snapshot(&manual, &document());
        assert_eq!(view.status, "Paused manually");
        assert!(view.current_step.is_none());
        assert_eq!(view.variable_label, "Last safe execution boundary");

        for (state, reason, status) in [
            (
                RuntimeState::Stopping,
                DebugSnapshotReason::StepBoundary,
                "Stopping",
            ),
            (
                RuntimeState::Completed,
                DebugSnapshotReason::RunFinished,
                "Completed",
            ),
            (
                RuntimeState::Stopped,
                DebugSnapshotReason::RunCancelled,
                "Stopped",
            ),
            (
                RuntimeState::Failed,
                DebugSnapshotReason::RunFailed,
                "Failed",
            ),
        ] {
            let mut terminal = snapshot();
            terminal.state = state;
            terminal.debug_snapshot_reason = Some(reason);
            let view = RuntimeInspectorViewModel::from_snapshot(&terminal, &document());
            assert_eq!(view.status, status);
            let expected_title = if state == RuntimeState::Stopping {
                "Runtime Inspector — Current Debug Run"
            } else {
                "Runtime Inspector — Last Debug Run"
            };
            assert_eq!(view.title, expected_title);
            assert_eq!(
                view.snapshot_reason,
                debug_snapshot_reason_description(reason)
            );
        }
    }

    #[test]
    fn outcomes_use_step_details_or_keyed_failure_context() {
        let mut success = snapshot();
        success.state = RuntimeState::Completed;
        success.last_completed = Some(Arc::new(CompletedStep {
            frame: ExecutionFrameContext::root(7),
            key: MacroStepKey::new(7, 101),
            macro_name: Arc::from("Demo"),
            outcome: CompletedStepOutcome::Success(Some(StepOutcome {
                last_image_found: Some(true),
                ..StepOutcome::default()
            })),
        }));
        assert_eq!(
            RuntimeInspectorViewModel::from_snapshot(&success, &document()).outcome,
            Some(LastOutcomePresentation::Success {
                detail: Some("Success — image found.".into())
            })
        );

        let diagnostic =
            ExecutionDiagnostic::new(crate::mkmacro::DiagnosticKind::Backend, "operation failed")
                .context("Expected", "found")
                .context("Actual", "missing")
                .context("Variable", "target");
        let mut failed = success.clone();
        failed.state = RuntimeState::Failed;
        failed.last_completed = Some(Arc::new(CompletedStep {
            frame: ExecutionFrameContext::root(7),
            key: MacroStepKey::new(7, 101),
            macro_name: Arc::from("Demo"),
            outcome: CompletedStepOutcome::Failure(diagnostic.clone()),
        }));
        failed.latest_failure = Some(diagnostic);
        assert_eq!(
            RuntimeInspectorViewModel::from_snapshot(&failed, &document()).outcome,
            Some(LastOutcomePresentation::Failure {
                message: "operation failed".into(),
                context: vec![
                    ("Actual".into(), "missing".into()),
                    ("Expected".into(), "found".into()),
                    ("Variable".into(), "target".into()),
                ],
            })
        );
    }

    #[test]
    fn deleted_steps_macros_and_empty_completion_are_safe() {
        let mut missing = snapshot();
        missing.state = RuntimeState::Completed;
        missing.last_completed = Some(Arc::new(CompletedStep {
            frame: ExecutionFrameContext::root(7),
            key: MacroStepKey::new(7, 404),
            macro_name: Arc::from("Demo"),
            outcome: CompletedStepOutcome::Success(None),
        }));
        let view = RuntimeInspectorViewModel::from_snapshot(&missing, &document());
        assert_eq!(view.macro_name, "Demo");
        assert!(
            view.last_step
                .as_ref()
                .unwrap()
                .label()
                .contains("unavailable")
        );

        missing.macro_id = Some(999);
        missing.call_stack = Arc::new(Vec::new());
        let view = RuntimeInspectorViewModel::from_snapshot(&missing, &document());
        assert_eq!(view.macro_name, "Removed macro #999");
        assert!(view.last_step.is_some());

        missing.last_completed = None;
        let view = RuntimeInspectorViewModel::from_snapshot(&missing, &document());
        assert!(view.last_step.is_none());
        assert!(view.outcome.is_none());
    }

    #[test]
    fn last_completion_keeps_callee_identity_after_pop_and_reentry() {
        let mut document = document();
        let mut child = document.macros[0].clone();
        child.id = 8;
        child.name = "edited child name".into();
        child.steps[0].action = MkAction::Text(crate::mkmacro::MkTextPayload {
            text: "child".into(),
            mode: crate::mkmacro::MkTextMode::Type,
        });
        document.macros.push(child);
        let mut snapshot = snapshot();
        snapshot.state = RuntimeState::Running;
        Arc::make_mut(&mut snapshot.call_stack)[0].active_step_id = Some(101);
        let child_context = ExecutionFrameContext {
            macro_id: 8,
            frame_id: 3,
            caller_step_id: Some(101),
            depth: 2,
        };
        Arc::make_mut(&mut snapshot.call_stack).push(crate::mkmacro::ExecutionFrameSnapshot {
            context: child_context,
            macro_name: Arc::from("Child at run time"),
            active_step_id: Some(101),
        });
        let key = MacroStepKey::new(8, 101);
        let original_failure = ExecutionDiagnostic::new(
            crate::mkmacro::DiagnosticKind::Backend,
            "previous invocation result",
        );
        snapshot.last_completed = Some(Arc::new(CompletedStep {
            frame: ExecutionFrameContext {
                frame_id: 2,
                ..child_context
            },
            key,
            macro_name: Arc::from("Original child name"),
            outcome: CompletedStepOutcome::Failure(original_failure),
        }));
        snapshot.macro_steps =
            Arc::new(BTreeMap::from([(key, crate::mkmacro::StepState::Pending)]));
        snapshot.macro_failures = Arc::new(BTreeMap::from([(
            crate::mkmacro::MacroDiagnosticKey {
                run_id: 9,
                step: key,
            },
            ExecutionDiagnostic::new(
                crate::mkmacro::DiagnosticKind::Backend,
                "different invocation",
            ),
        )]));
        let view = RuntimeInspectorViewModel::from_snapshot(&snapshot, &document);
        assert_eq!(view.macro_name, "Child at run time");
        assert_eq!(view.current_step.as_ref().unwrap().macro_id, 8);
        assert_eq!(view.last_step.as_ref().unwrap().label(), "#1 Text");
        assert_eq!(
            view.last_step_macro_name.as_deref(),
            Some("Original child name")
        );
        assert!(
            matches!(view.outcome, Some(LastOutcomePresentation::Failure { message, .. }) if message == "previous invocation result")
        );
        assert_eq!(view.call_stack.len(), 2);
        assert!(!view.call_stack[0].active);
        assert!(view.call_stack[1].active);
        Arc::make_mut(&mut snapshot.call_stack).pop();
        let restored = RuntimeInspectorViewModel::from_snapshot(&snapshot, &document);
        assert_eq!(restored.current_step.as_ref().unwrap().macro_id, 7);
        assert_eq!(restored.current_step.as_ref().unwrap().label(), "#1 Delay");
        assert_eq!(restored.last_step.as_ref().unwrap().macro_id, 8);
        assert_eq!(restored.last_step.as_ref().unwrap().label(), "#1 Text");
        document.macros.retain(|owner| owner.id == 7);
        let removed = RuntimeInspectorViewModel::from_snapshot(&snapshot, &document);
        assert!(
            removed
                .last_step
                .as_ref()
                .unwrap()
                .label()
                .contains("unavailable")
        );
        assert_eq!(
            removed.last_step_macro_name.as_deref(),
            Some("Original child name")
        );
    }

    #[test]
    fn inspector_body_height_stays_within_requested_cap() {
        assert!((150.0..=220.0).contains(&inspector_body_height()));
    }

    #[test]
    fn controlled_header_state_ignores_animation_progress() {
        let mut open = false;

        for _ in 0..3 {
            apply_header_intent(&mut open, false);
        }
        assert!(!open);

        apply_header_intent(&mut open, true);
        assert!(open);
        for _ in 0..3 {
            apply_header_intent(&mut open, false);
        }
        assert!(open);

        apply_header_intent(&mut open, true);
        assert!(!open);
    }

    #[test]
    fn controlled_header_intents_are_independent_from_each_other_and_visibility() {
        let mut outer_open = false;
        let mut builtins_open = false;
        let mut show_internal = false;
        assert!(!show_internal);

        apply_header_intent(&mut builtins_open, true);
        apply_header_intent(&mut outer_open, false);
        assert!(!outer_open);
        assert!(builtins_open);

        // An outer-header activation (including breakpoint auto-opening) does
        // not alter the Built-ins expansion or internal-variable preference.
        apply_header_intent(&mut outer_open, true);
        show_internal = true;
        apply_header_intent(&mut outer_open, false);
        apply_header_intent(&mut builtins_open, false);
        assert!(outer_open);
        assert!(builtins_open);
        assert!(show_internal);
    }
}
