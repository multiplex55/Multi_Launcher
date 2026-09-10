//! Transient recorder review state and UI.
//!
//! This module deliberately owns no persisted document state. Recorder output is
//! copied into an immutable literal/cleaned baseline and becomes a document
//! mutation only when the dialog applies the complete proposal.

use super::MkMacroDialog;
use crate::mkmacro::{
    PlannedStep, RecordedStep, RecordingPlan, RecordingResult, RecordingSourceSpan,
    RecordingSuggestion, RecordingSuggestionId, RecordingTarget, ReviewStepId,
    SuggestionApplicationError, apply_suggestions,
};
use std::{
    collections::{BTreeSet, HashSet},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingReviewMode {
    Generated,
    Editable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingReviewView {
    Literal,
    Cleaned,
    Actions,
}

#[derive(Debug, Clone, PartialEq)]
struct ReviewEditSnapshot {
    steps: Vec<PlannedStep>,
    selection: BTreeSet<ReviewStepId>,
    primary: Option<ReviewStepId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordingReviewStatistics {
    pub duration: Duration,
    pub raw_event_count: u64,
    pub literal_action_count: usize,
    pub cleaned_action_count: usize,
    pub generated_action_count: usize,
    pub reduction_count: usize,
    pub reduction_percent: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingReviewError {
    UnknownSuggestion(RecordingSuggestionId),
    Suggestion(SuggestionApplicationError),
    NoSelection,
    MissingReviewStep(ReviewStepId),
    ManualEditsRequireConfirmation,
}

impl std::fmt::Display for RecordingReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSuggestion(id) => write!(f, "Suggestion {} no longer exists", id.0),
            Self::Suggestion(error) => write!(f, "{error:?}"),
            Self::NoSelection => f.write_str("Select one or more reviewed actions"),
            Self::MissingReviewStep(id) => write!(f, "Reviewed action {} no longer exists", id.0),
            Self::ManualEditsRequireConfirmation => {
                f.write_str("Reconfiguring cleanup will discard manual review edits")
            }
        }
    }
}

impl std::error::Error for RecordingReviewError {}

/// One capture's complete, process-local review state.
///
/// `literal_steps` and `cleaned_plan` never change. Generated mode rebuilds
/// `generated_plan` atomically. Editable mode owns a detached working copy and a
/// deliberately small local history, so review operations cannot dirty the
/// document draft.
pub struct RecordingReviewSession {
    pub target: RecordingTarget,
    recovery_target: Option<RecordingTarget>,
    pub literal_steps: Vec<RecordedStep>,
    pub cleaned_plan: RecordingPlan,
    pub suggestions: Vec<RecordingSuggestion>,
    pub enabled_suggestions: HashSet<RecordingSuggestionId>,
    generated_plan: RecordingPlan,
    editable_steps: Option<Vec<PlannedStep>>,
    pub mode: RecordingReviewMode,
    pub view: RecordingReviewView,
    pub selection: BTreeSet<ReviewStepId>,
    pub primary: Option<ReviewStepId>,
    undo: Vec<ReviewEditSnapshot>,
    redo: Vec<ReviewEditSnapshot>,
    pub statistics: RecordingReviewStatistics,
    pub dropped_event_count: u64,
    pub clipboard_observations: Vec<crate::mkmacro::ClipboardObservation>,
    pub click_inspections: Vec<crate::mkmacro::ClickInspection>,
    pub window_observations: Vec<crate::mkmacro::WindowObservation>,
    pub notes: Vec<crate::mkmacro::RecordingNote>,
    pub reconfigure_confirmation: bool,
    pub message: Option<String>,
    /// Ticket of the last preview submitted from this review. It is used only
    /// to stop/label that exact run and is never persisted.
    pub preview_ticket: Option<u64>,
    pub preview_state: Option<crate::mkmacro::RuntimeState>,
    pub preview_diagnostic: Option<crate::mkmacro::ExecutionDiagnostic>,
}

impl RecordingReviewSession {
    pub fn new(result: RecordingResult) -> Self {
        let RecordingResult {
            target,
            literal_steps,
            plan: cleaned_plan,
            suggestions,
            clipboard_observations,
            click_inspections,
            window_observations,
            notes,
            dropped_event_count,
            capture_duration,
            raw_event_count,
            ..
        } = result;
        let mut enabled = HashSet::new();
        // Accept defaults one at a time. This gives overlapping producer output
        // deterministic conflict handling without ever losing the review.
        for suggestion in suggestions.iter().filter(|s| s.enabled_by_default) {
            let mut candidate = enabled.clone();
            candidate.insert(suggestion.id);
            if apply_suggestions(&cleaned_plan, &suggestions, &candidate).is_ok() {
                enabled = candidate;
            }
        }
        let mut generated_plan = apply_suggestions(&cleaned_plan, &suggestions, &enabled)
            .unwrap_or_else(|_| cleaned_plan.clone());
        assign_review_ids(&mut generated_plan.steps);
        let statistics = statistics(
            capture_duration,
            raw_event_count,
            literal_steps.len(),
            cleaned_plan.steps.len(),
            generated_plan.steps.len(),
        );
        Self {
            target,
            recovery_target: None,
            literal_steps,
            cleaned_plan,
            suggestions,
            enabled_suggestions: enabled,
            generated_plan,
            editable_steps: None,
            mode: RecordingReviewMode::Generated,
            view: RecordingReviewView::Actions,
            selection: BTreeSet::new(),
            primary: None,
            undo: Vec::new(),
            redo: Vec::new(),
            statistics,
            dropped_event_count,
            clipboard_observations,
            click_inspections,
            window_observations,
            notes,
            reconfigure_confirmation: false,
            message: None,
            preview_ticket: None,
            preview_state: None,
            preview_diagnostic: None,
        }
    }

    pub fn effective_target(&self) -> RecordingTarget {
        self.recovery_target.unwrap_or(self.target)
    }

    pub fn recover_append_to_original_macro(&mut self) {
        self.recovery_target = Some(RecordingTarget {
            macro_id: self.target.macro_id,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        });
        self.message = None;
    }

    pub fn retarget_append(&mut self, macro_id: u64) {
        self.recovery_target = Some(RecordingTarget {
            macro_id,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        });
        self.message = None;
    }

    pub fn proposed_steps(&self) -> &[PlannedStep] {
        self.editable_steps
            .as_deref()
            .unwrap_or(&self.generated_plan.steps)
    }

    pub fn proposal_plan(&self) -> RecordingPlan {
        RecordingPlan {
            steps: self.proposed_steps().to_vec(),
        }
    }

    /// Preview selection is a contiguous timeline slice from the first to the
    /// last selected row. Disjoint selections therefore retain intervening
    /// delays/actions instead of silently changing recorded timing.
    pub fn preview_plan(
        &self,
        selected_range: bool,
    ) -> Result<RecordingPlan, RecordingReviewError> {
        if !selected_range {
            return Ok(self.proposal_plan());
        }
        let (first, last) = self.selected_bounds()?;
        Ok(RecordingPlan {
            steps: self.proposed_steps()[first..=last].to_vec(),
        })
    }

    pub fn observe_preview(&mut self, snapshot: &crate::mkmacro::RuntimeSnapshot) {
        let Some(ticket) = self.preview_ticket else {
            return;
        };
        if snapshot.origin != (crate::mkmacro::RuntimeOrigin::RecordingPreview { ticket }) {
            return;
        }
        self.preview_state = Some(snapshot.state);
        if snapshot.state == crate::mkmacro::RuntimeState::Failed {
            self.preview_diagnostic = snapshot.latest_failure.clone();
        }
    }

    pub fn toggle_suggestion(
        &mut self,
        id: RecordingSuggestionId,
        enabled: bool,
    ) -> Result<(), RecordingReviewError> {
        if !self
            .suggestions
            .iter()
            .any(|suggestion| suggestion.id == id)
        {
            return Err(RecordingReviewError::UnknownSuggestion(id));
        }
        if self.mode == RecordingReviewMode::Editable {
            return Err(RecordingReviewError::ManualEditsRequireConfirmation);
        }
        let mut candidate = self.enabled_suggestions.clone();
        if enabled {
            candidate.insert(id);
        } else {
            candidate.remove(&id);
        }
        let mut plan = apply_suggestions(&self.cleaned_plan, &self.suggestions, &candidate)
            .map_err(RecordingReviewError::Suggestion)?;
        assign_review_ids(&mut plan.steps);
        self.enabled_suggestions = candidate;
        self.generated_plan = plan;
        self.clear_selection();
        self.update_generated_statistics();
        Ok(())
    }

    pub fn select(&mut self, id: ReviewStepId, toggle: bool, extend: bool) {
        let Some(clicked_index) = self
            .proposed_steps()
            .iter()
            .position(|step| step.review_id == id)
        else {
            return;
        };
        if extend
            && let Some(primary) = self.primary
            && let Some(primary_index) = self
                .proposed_steps()
                .iter()
                .position(|step| step.review_id == primary)
        {
            if !toggle {
                self.selection.clear();
            }
            let (first, last) = if primary_index <= clicked_index {
                (primary_index, clicked_index)
            } else {
                (clicked_index, primary_index)
            };
            let range: Vec<_> = self.proposed_steps()[first..=last]
                .iter()
                .map(|step| step.review_id)
                .collect();
            self.selection.extend(range);
        } else if toggle {
            if !self.selection.remove(&id) {
                self.selection.insert(id);
            }
        } else {
            self.selection.clear();
            self.selection.insert(id);
        }
        self.primary = self.selection.contains(&id).then_some(id);
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.primary = None;
    }

    pub fn delete_selected(&mut self) -> Result<(), RecordingReviewError> {
        if self.selection.is_empty() {
            return Err(RecordingReviewError::NoSelection);
        }
        self.begin_edit_operation();
        let selected = self.selection.clone();
        self.editable_steps
            .as_mut()
            .unwrap()
            .retain(|step| !selected.contains(&step.review_id));
        self.clear_selection();
        self.update_generated_statistics();
        Ok(())
    }

    pub fn trim_before_selection(&mut self) -> Result<(), RecordingReviewError> {
        let first = self.selected_bounds()?.0;
        self.begin_edit_operation();
        self.editable_steps.as_mut().unwrap().drain(..first);
        self.clear_selection();
        self.update_generated_statistics();
        Ok(())
    }

    pub fn trim_after_selection(&mut self) -> Result<(), RecordingReviewError> {
        let last = self.selected_bounds()?.1;
        self.begin_edit_operation();
        self.editable_steps.as_mut().unwrap().truncate(last + 1);
        self.clear_selection();
        self.update_generated_statistics();
        Ok(())
    }

    pub fn replace_step(
        &mut self,
        id: ReviewStepId,
        edited: &crate::mkmacro::MkStep,
    ) -> Result<(), RecordingReviewError> {
        if !self
            .proposed_steps()
            .iter()
            .any(|step| step.review_id == id)
        {
            return Err(RecordingReviewError::MissingReviewStep(id));
        }
        self.begin_edit_operation();
        let step = self
            .editable_steps
            .as_mut()
            .unwrap()
            .iter_mut()
            .find(|step| step.review_id == id)
            .ok_or(RecordingReviewError::MissingReviewStep(id))?;
        step.action = edited.action.clone();
        step.enabled = edited.enabled;
        step.breakpoint = edited.breakpoint;
        step.repeat = edited.repeat.max(1);
        step.delay_after_ms = edited.delay_after_ms;
        step.on_error = edited.on_error.clone();
        step.metadata = edited.metadata.clone();
        self.update_generated_statistics();
        Ok(())
    }

    pub fn step_for_editor(&self, id: ReviewStepId) -> Option<crate::mkmacro::MkStep> {
        let step = self
            .proposed_steps()
            .iter()
            .find(|step| step.review_id == id)?;
        Some(crate::mkmacro::MkStep {
            id: id.0,
            enabled: step.enabled,
            breakpoint: step.breakpoint,
            repeat: step.repeat,
            delay_after_ms: step.delay_after_ms,
            on_error: step.on_error.clone(),
            metadata: step.metadata.clone(),
            action: step.action.clone(),
        })
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn reconfigure(&mut self, discard_edits: bool) -> Result<(), RecordingReviewError> {
        if self.mode == RecordingReviewMode::Editable && !discard_edits {
            return Err(RecordingReviewError::ManualEditsRequireConfirmation);
        }
        self.mode = RecordingReviewMode::Generated;
        self.editable_steps = None;
        self.undo.clear();
        self.redo.clear();
        self.clear_selection();
        self.reconfigure_confirmation = false;
        self.update_generated_statistics();
        Ok(())
    }

    pub fn clear_sensitive(&mut self) {
        self.clipboard_observations.clear();
        self.click_inspections.clear();
        self.window_observations.clear();
        self.notes.clear();
    }

    fn selected_bounds(&self) -> Result<(usize, usize), RecordingReviewError> {
        let mut selected = self
            .proposed_steps()
            .iter()
            .enumerate()
            .filter(|(_, step)| self.selection.contains(&step.review_id))
            .map(|(index, _)| index);
        let first = selected.next().ok_or(RecordingReviewError::NoSelection)?;
        Ok((first, selected.last().unwrap_or(first)))
    }

    fn begin_edit_operation(&mut self) {
        if self.mode == RecordingReviewMode::Generated {
            self.editable_steps = Some(self.generated_plan.steps.clone());
            self.mode = RecordingReviewMode::Editable;
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
    }

    fn snapshot(&self) -> ReviewEditSnapshot {
        ReviewEditSnapshot {
            steps: self.proposed_steps().to_vec(),
            selection: self.selection.clone(),
            primary: self.primary,
        }
    }

    fn restore(&mut self, snapshot: ReviewEditSnapshot) {
        self.mode = RecordingReviewMode::Editable;
        self.editable_steps = Some(snapshot.steps);
        self.selection = snapshot.selection;
        self.primary = snapshot.primary;
        self.update_generated_statistics();
    }

    fn update_generated_statistics(&mut self) {
        self.statistics.generated_action_count = self.proposed_steps().len();
        let base = self.statistics.literal_action_count;
        self.statistics.reduction_count =
            base.saturating_sub(self.statistics.generated_action_count);
        self.statistics.reduction_percent = if base == 0 {
            0.0
        } else {
            self.statistics.reduction_count as f32 * 100.0 / base as f32
        };
    }
}

fn statistics(
    duration: Duration,
    raw_event_count: u64,
    literal_action_count: usize,
    cleaned_action_count: usize,
    generated_action_count: usize,
) -> RecordingReviewStatistics {
    let reduction_count = literal_action_count.saturating_sub(generated_action_count);
    RecordingReviewStatistics {
        duration,
        raw_event_count,
        literal_action_count,
        cleaned_action_count,
        generated_action_count,
        reduction_count,
        reduction_percent: if literal_action_count == 0 {
            0.0
        } else {
            reduction_count as f32 * 100.0 / literal_action_count as f32
        },
    }
}

fn assign_review_ids(steps: &mut [PlannedStep]) {
    for (index, step) in steps.iter_mut().enumerate() {
        step.review_id = ReviewStepId(index as u64 + 1);
    }
}

fn provenance_label(step: &PlannedStep) -> String {
    format!(
        "{:?} · source {}–{}",
        step.provenance,
        step.source.first + 1,
        step.source.last + 1
    )
}

pub fn show(ctx: &eframe::egui::Context, dialog: &mut MkMacroDialog) {
    if dialog.recording_review.is_none() {
        return;
    }
    let mut open = true;
    let mut cancel = false;
    let mut apply = false;
    let mut edit = None;
    let mut append_recovery = false;
    let mut retarget = None;
    let mut preview_all = false;
    let mut preview_selected = false;
    let mut stop_preview = false;
    let review_editor_open = dialog.action_editor.review_editing_id().is_some();
    let apply_blocker = dialog.recording_review_apply_blocker();
    let runtime = crate::mkmacro::runtime::snapshot();
    let preview_ticket = dialog
        .recording_review
        .as_ref()
        .and_then(|review| review.preview_ticket);
    let (preview_active, preview_snapshot) = preview_ticket.map_or((false, None), |ticket| {
        crate::mkmacro::runtime::recording_preview_status(ticket)
    });
    if let (Some(review), Some(snapshot)) = (
        dialog.recording_review.as_mut(),
        preview_snapshot.as_deref(),
    ) {
        review.observe_preview(snapshot);
    } else if preview_ticket.is_some() && !preview_active {
        // A runtime replacement invalidates an accepted ticket. Do not leave
        // Review permanently displaying the optimistic Previewing state.
        dialog.recording_review.as_mut().unwrap().preview_state =
            Some(crate::mkmacro::RuntimeState::Stopped);
    }
    let playback_active = runtime.as_deref().is_some_and(|snapshot| {
        matches!(
            snapshot.state,
            crate::mkmacro::RuntimeState::Running
                | crate::mkmacro::RuntimeState::Paused
                | crate::mkmacro::RuntimeState::Stopping
        )
    });
    eframe::egui::Window::new("Recording Review")
        .id(eframe::egui::Id::new("mkmacro_recording_review"))
        .open(&mut open)
        .collapsible(false)
        .default_size([780.0, 620.0])
        .resizable(true)
        .show(ctx, |ui| {
            let review = dialog.recording_review.as_mut().unwrap();
            let stats = &review.statistics;
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "{} raw · {} literal → {} cleaned → {} proposed",
                    stats.raw_event_count,
                    stats.literal_action_count,
                    stats.cleaned_action_count,
                    stats.generated_action_count
                ));
                ui.label(format!(
                    "{:.1}% reduction · {:.2}s",
                    stats.reduction_percent,
                    stats.duration.as_secs_f32()
                ));
            });
            if review.dropped_event_count > 0 {
                ui.colored_label(
                    eframe::egui::Color32::YELLOW,
                    format!("{} input events were dropped", review.dropped_event_count),
                );
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut review.view, RecordingReviewView::Literal, "Literal");
                ui.selectable_value(&mut review.view, RecordingReviewView::Cleaned, "Cleaned");
                ui.selectable_value(&mut review.view, RecordingReviewView::Actions, "Actions");
                ui.separator();
                ui.label(match review.mode {
                    RecordingReviewMode::Generated => "Generated mode",
                    RecordingReviewMode::Editable => "Editable mode",
                });
            });
            ui.separator();

            if !review.suggestions.is_empty() {
                ui.collapsing("Smart transformations", |ui| {
                    let suggestions: Vec<_> = review
                        .suggestions
                        .iter()
                        .map(|suggestion| {
                            (
                                suggestion.id,
                                suggestion.description.clone(),
                                suggestion.rationale.clone(),
                                suggestion.confidence,
                                review.enabled_suggestions.contains(&suggestion.id),
                            )
                        })
                        .collect();
                    for (id, description, rationale, confidence, mut enabled) in suggestions {
                        let response = ui.add_enabled(
                            review.mode == RecordingReviewMode::Generated && !review_editor_open,
                            eframe::egui::Checkbox::new(
                                &mut enabled,
                                format!("{description} ({confidence:?})"),
                            ),
                        );
                        response.on_hover_text(rationale.clone());
                        ui.small(rationale);
                        if enabled != review.enabled_suggestions.contains(&id)
                            && let Err(error) = review.toggle_suggestion(id, enabled)
                        {
                            review.message = Some(error.to_string());
                        }
                    }
                });
            }

            if let Some(id) = review.primary
                && let Some(step) = review
                    .proposed_steps()
                    .iter()
                    .find(|step| step.review_id == id)
            {
                ui.collapsing("Selected action details", |ui| {
                    ui.label(provenance_label(step));
                    ui.label(format!(
                        "Delay after: {} ms · Repeat: {}",
                        step.delay_after_ms, step.repeat
                    ));
                    if step.metadata.bookmarked {
                        ui.label("Marker: bookmarked");
                    }
                    if !step.metadata.comment.is_empty() {
                        ui.label(format!("Annotation: {}", step.metadata.comment));
                    }
                    if let Some((first, last)) = review
                        .literal_steps
                        .get(step.source.first)
                        .zip(review.literal_steps.get(step.source.last))
                    {
                        ui.label(format!(
                            "Captured at {:.3}–{:.3}s",
                            first.timestamp_us as f64 / 1_000_000.0,
                            last.timestamp_us as f64 / 1_000_000.0
                        ));
                        if let Some(context) = review
                            .literal_steps
                            .get(step.association_source)
                            .or_else(|| review.literal_steps.get(step.source.first))
                            .and_then(|literal| literal.context.as_ref())
                        {
                            let target = context
                                .window_under_point
                                .as_ref()
                                .unwrap_or(&context.foreground);
                            ui.label(format!(
                                "Window: {} · {} · {}",
                                target.executable, target.title, target.class
                            ));
                        }
                        let click_timestamp =
                            matches!(step.action, crate::mkmacro::MkAction::MouseClick(_))
                                .then(|| {
                                    review.literal_steps[step.source.first..=step.source.last]
                                        .iter()
                                        .find(|literal| {
                                            matches!(
                                                literal.action,
                                                crate::mkmacro::RecordedAction::Click { .. }
                                            )
                                        })
                                        .map(|literal| literal.timestamp_us)
                                })
                                .flatten();
                        if let Some((inspection, click_timestamp)) =
                            click_timestamp.and_then(|timestamp| {
                                review
                                    .click_inspections
                                    .iter()
                                    .min_by_key(|inspection| {
                                        inspection.timestamp_us.abs_diff(timestamp)
                                    })
                                    .filter(|inspection| {
                                        inspection.timestamp_us.abs_diff(timestamp) <= 1_000_000
                                    })
                                    .map(|inspection| (inspection, timestamp))
                            })
                        {
                            let selector = &inspection.info.selector;
                            ui.separator();
                            ui.strong("Detected UI control");
                            ui.label(format!("Type: {:?}", selector.control_type));
                            ui.label(format!("Name: {}", inspection.info.user_facing_name));
                            ui.label(format!(
                                "AutomationId: {}",
                                selector.automation_id.as_deref().unwrap_or("—")
                            ));
                            ui.label(format!(
                                "Class: {}",
                                selector.class_name.as_deref().unwrap_or("—")
                            ));
                            ui.label(format!(
                                "Framework: {}",
                                selector.framework_id.as_deref().unwrap_or("—")
                            ));
                            ui.label(format!(
                                "Supported: {:?}",
                                inspection.info.supported_patterns
                            ));
                            ui.small(format!(
                                "Matched click at {:.3}s",
                                click_timestamp as f64 / 1_000_000.0
                            ));
                        }
                    }
                });
            }
            if !review.notes.is_empty() {
                ui.collapsing("Markers and annotations", |ui| {
                    for note in &review.notes {
                        match note {
                            crate::mkmacro::RecordingNote::Marker { timestamp_us } => {
                                ui.label(format!(
                                    "Marker at {:.3}s",
                                    *timestamp_us as f64 / 1_000_000.0
                                ));
                            }
                            crate::mkmacro::RecordingNote::Annotation { timestamp_us, text } => {
                                ui.label(format!(
                                    "Annotation at {:.3}s: {text}",
                                    *timestamp_us as f64 / 1_000_000.0
                                ));
                            }
                        }
                    }
                });
            }

            let height = ui.available_height().max(160.0) - 130.0;
            eframe::egui::ScrollArea::vertical()
                .max_height(height.max(120.0))
                .show(ui, |ui| match review.view {
                    RecordingReviewView::Literal => {
                        for (index, step) in review.literal_steps.iter().enumerate() {
                            ui.label(format!(
                                "{}. {:?} · +{} ms",
                                index + 1,
                                step.action,
                                step.delay_after_ms
                            ));
                        }
                    }
                    RecordingReviewView::Cleaned => {
                        for (index, step) in review.cleaned_plan.steps.iter().enumerate() {
                            ui.label(format!(
                                "{}. {} — {}",
                                index + 1,
                                super::action_catalog::action_name(&step.action),
                                provenance_label(step)
                            ));
                        }
                    }
                    RecordingReviewView::Actions => {
                        let mut selection_request = None;
                        for (index, step) in review.proposed_steps().iter().enumerate() {
                            let selected = review.selection.contains(&step.review_id);
                            let label = format!(
                                "{}. {} — {}",
                                index + 1,
                                super::action_catalog::action_name(&step.action),
                                super::action_catalog::action_details(&step.action)
                            );
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                let modifiers = ui.input(|input| input.modifiers);
                                selection_request = Some((
                                    step.review_id,
                                    modifiers.ctrl || modifiers.command,
                                    modifiers.shift,
                                ));
                            }
                            response.on_hover_text(provenance_label(step));
                        }
                        if let Some((id, toggle, extend)) = selection_request {
                            review.select(id, toggle, extend);
                        }
                    }
                });

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let selected = !review.selection.is_empty();
                if ui
                    .add_enabled(
                        selected && !review_editor_open,
                        eframe::egui::Button::new("Delete selected"),
                    )
                    .clicked()
                    && let Err(error) = review.delete_selected()
                {
                    review.message = Some(error.to_string());
                }
                if ui
                    .add_enabled(
                        selected && !review_editor_open,
                        eframe::egui::Button::new("Trim before"),
                    )
                    .clicked()
                    && let Err(error) = review.trim_before_selection()
                {
                    review.message = Some(error.to_string());
                }
                if ui
                    .add_enabled(
                        selected && !review_editor_open,
                        eframe::egui::Button::new("Trim after"),
                    )
                    .clicked()
                    && let Err(error) = review.trim_after_selection()
                {
                    review.message = Some(error.to_string());
                }
                let one =
                    (review.selection.len() == 1).then(|| *review.selection.iter().next().unwrap());
                if ui
                    .add_enabled(
                        one.is_some() && !review_editor_open,
                        eframe::egui::Button::new("Edit action"),
                    )
                    .clicked()
                {
                    edit = one;
                }
                if ui
                    .add_enabled(
                        review.can_undo() && !review_editor_open,
                        eframe::egui::Button::new("Undo review edit"),
                    )
                    .clicked()
                {
                    review.undo();
                }
                if ui
                    .add_enabled(
                        review.can_redo() && !review_editor_open,
                        eframe::egui::Button::new("Redo review edit"),
                    )
                    .clicked()
                {
                    review.redo();
                }
                if ui
                    .add_enabled(
                        !review_editor_open,
                        eframe::egui::Button::new("Reconfigure cleanup"),
                    )
                    .clicked()
                {
                    if review.mode == RecordingReviewMode::Editable {
                        review.reconfigure_confirmation = true;
                    } else {
                        let _ = review.reconfigure(true);
                    }
                }
            });
            if review.reconfigure_confirmation {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        eframe::egui::Color32::YELLOW,
                        "Discard manual review edits and rebuild from cleanup settings?",
                    );
                    if ui.button("Discard edits").clicked() {
                        let _ = review.reconfigure(true);
                    }
                    if ui.button("Keep editing").clicked() {
                        review.reconfigure_confirmation = false;
                    }
                });
            }
            if let Some(message) = &review.message {
                ui.colored_label(eframe::egui::Color32::YELLOW, message);
            }
            if let Some(diagnostic) = &review.preview_diagnostic {
                ui.colored_label(
                    eframe::egui::Color32::YELLOW,
                    format!("Preview failed: {}", diagnostic.message),
                );
                for (key, value) in &diagnostic.context {
                    ui.small(format!("{key}: {value}"));
                }
            }
            if let Some(state) = review.preview_state {
                let label = match state {
                    crate::mkmacro::RuntimeState::Running => "Previewing",
                    crate::mkmacro::RuntimeState::Paused => "Preview Paused",
                    crate::mkmacro::RuntimeState::Stopping => "Preview Stopping",
                    crate::mkmacro::RuntimeState::Completed => "Preview Completed",
                    crate::mkmacro::RuntimeState::Stopped => "Preview Stopped",
                    crate::mkmacro::RuntimeState::Failed => "Preview Failed",
                    crate::mkmacro::RuntimeState::Idle => "Preview Idle",
                };
                ui.label(label);
            }
            if let Some(blocker) = &apply_blocker {
                if review.message.as_deref() != Some(blocker) {
                    ui.colored_label(eframe::egui::Color32::YELLOW, blocker);
                }
                if blocker.contains("original insertion step")
                    && ui
                        .add_enabled(
                            !review_editor_open,
                            eframe::egui::Button::new("Append to end"),
                        )
                        .clicked()
                {
                    append_recovery = true;
                }
                if blocker.contains("target macro") {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Retarget and append:");
                        for macro_item in &dialog.draft.macros {
                            ui.push_id(macro_item.id, |ui| {
                                if ui
                                    .add_enabled(
                                        !review_editor_open,
                                        eframe::egui::Button::new(format!(
                                            "{} (#{})",
                                            macro_item.name, macro_item.id
                                        )),
                                    )
                                    .clicked()
                                {
                                    retarget = Some(macro_item.id);
                                }
                            });
                        }
                    });
                }
            }
            ui.horizontal(|ui| {
                if preview_active {
                    if ui.button("Stop Preview").clicked() {
                        stop_preview = true;
                    }
                } else {
                    if ui
                        .add_enabled(
                            !review_editor_open && !playback_active && !review.proposed_steps().is_empty(),
                            eframe::egui::Button::new("Play All"),
                        )
                        .clicked()
                    {
                        preview_all = true;
                    }
                    if ui
                        .add_enabled(
                            !review_editor_open && !playback_active && !review.selection.is_empty(),
                            eframe::egui::Button::new("Play Selected Range"),
                        )
                        .on_hover_text("Plays from the first through the last selected action, including intervening rows")
                        .clicked()
                    {
                        preview_selected = true;
                    }
                }
                if ui
                    .add_enabled(
                        !review_editor_open && apply_blocker.is_none(),
                        eframe::egui::Button::new("Apply"),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .add_enabled(!review_editor_open, eframe::egui::Button::new("Cancel"))
                    .clicked()
                {
                    cancel = true;
                }
            });
            if review_editor_open {
                ui.small("Finish or cancel the Action Editor before changing this review.");
            }
        });

    if let Some(id) = edit
        && dialog.action_editor.draft.is_none()
        && let Some(step) = dialog
            .recording_review
            .as_ref()
            .and_then(|review| review.step_for_editor(id))
    {
        let macro_id = dialog
            .recording_review
            .as_ref()
            .unwrap()
            .effective_target()
            .macro_id;
        dialog.action_editor.begin_review_edit(macro_id, id, &step);
    }
    if append_recovery {
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .recover_append_to_original_macro();
    }
    if let Some(macro_id) = retarget {
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .retarget_append(macro_id);
    }
    if stop_preview {
        dialog.stop_owned_recording_preview();
    }
    if preview_all && let Err(error) = dialog.preview_recording_review(false) {
        dialog.recording_review.as_mut().unwrap().message = Some(error.to_string());
    }
    if preview_selected && let Err(error) = dialog.preview_recording_review(true) {
        dialog.recording_review.as_mut().unwrap().message = Some(error.to_string());
    }
    if apply {
        if let Err(error) = dialog.apply_recording_review() {
            if let Some(review) = &mut dialog.recording_review {
                review.message = Some(error);
            }
        }
    }
    if cancel || (!open && !review_editor_open) {
        dialog.cancel_recording_review();
    } else if !open && review_editor_open {
        dialog.recording_review.as_mut().unwrap().message =
            Some("Finish or cancel the Action Editor before closing Recording Review.".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        ClickInspection, MkAction, MkDelayPayload, MkUiControlType, MkUiSelector, RecordedAction,
        RecordedStep, RecordingProvenance, RecordingSuggestionKind, SuggestionConfidence,
        UiElementInfo,
    };

    fn planned(id: u64, source: usize) -> PlannedStep {
        PlannedStep {
            review_id: ReviewStepId(id),
            source: RecordingSourceSpan {
                first: source,
                last: source,
            },
            association_source: source,
            provenance: RecordingProvenance::Literal,
            action: MkAction::Delay(MkDelayPayload::default()),
            enabled: true,
            breakpoint: false,
            delay_after_ms: 0,
            repeat: 1,
            on_error: crate::mkmacro::MkErrorPolicy::Stop,
            metadata: Default::default(),
        }
    }

    fn click_planned(id: u64, source: usize) -> PlannedStep {
        PlannedStep {
            action: MkAction::MouseClick(crate::mkmacro::MkMousePayload {
                target: crate::mkmacro::MkCoordinateTarget::Screen {
                    point: crate::mkmacro::MkPoint { x: 4, y: 5 },
                },
                button: crate::mkmacro::MkMouseButton::Left,
                clicks: 1,
            }),
            ..planned(id, source)
        }
    }

    #[test]
    fn selected_preview_spans_first_through_last_selected_row() {
        let mut recording = result();
        recording.plan.steps.extend([planned(4, 3), planned(5, 4)]);
        recording.suggestions.clear();
        let mut review = RecordingReviewSession::new(recording);
        let ids: Vec<_> = review
            .proposed_steps()
            .iter()
            .map(|step| step.review_id)
            .collect();
        let all_sources: Vec<_> = review
            .proposed_steps()
            .iter()
            .map(|step| step.source.first)
            .collect();
        review.select(ids[1], false, false);
        review.select(ids[3], true, false);
        let preview = review.preview_plan(true).unwrap();
        assert_eq!(preview.steps.len(), 3);
        assert_eq!(
            preview
                .steps
                .iter()
                .map(|step| step.source.first)
                .collect::<Vec<_>>(),
            all_sources[1..=3]
        );
        assert_eq!(review.preview_plan(false).unwrap().steps.len(), 5);
    }

    fn result() -> RecordingResult {
        let plan = RecordingPlan {
            steps: vec![click_planned(1, 0), click_planned(2, 1), planned(3, 2)],
        };
        RecordingResult {
            target: RecordingTarget {
                macro_id: 7,
                insertion_anchor_step_id: Some(8),
                insertion_anchor_generation: None,
            },
            literal_steps: vec![],
            suggestions: vec![RecordingSuggestion {
                id: RecordingSuggestionId(1),
                kind: RecordingSuggestionKind::RepeatedClick,
                confidence: SuggestionConfidence::High,
                source_span: RecordingSourceSpan { first: 0, last: 1 },
                description: "combine".into(),
                rationale: "test".into(),
                enabled_by_default: true,
                replacement: vec![click_planned(0, 0)].into(),
            }],
            plan,
            clipboard_observations: vec![],
            click_inspections: vec![],
            window_observations: vec![],
            notes: vec![],
            dropped_event_count: 4,
            capture_duration: Duration::from_secs(2),
            raw_event_count: 12,
        }
    }

    #[test]
    fn generated_toggle_rebuilds_and_manual_edit_freezes_until_confirmed() {
        let mut review = RecordingReviewSession::new(result());
        assert_eq!(review.proposed_steps().len(), 2);
        review
            .toggle_suggestion(RecordingSuggestionId(1), false)
            .unwrap();
        assert_eq!(review.proposed_steps().len(), 3);
        let id = review.proposed_steps()[1].review_id;
        review.select(id, false, false);
        review.delete_selected().unwrap();
        assert_eq!(review.mode, RecordingReviewMode::Editable);
        assert_eq!(review.proposed_steps().len(), 2);
        assert!(matches!(
            review.toggle_suggestion(RecordingSuggestionId(1), true),
            Err(RecordingReviewError::ManualEditsRequireConfirmation)
        ));
        assert!(review.undo());
        assert_eq!(review.proposed_steps().len(), 3);
        assert!(matches!(
            review.reconfigure(false),
            Err(RecordingReviewError::ManualEditsRequireConfirmation)
        ));
        review.reconfigure(true).unwrap();
        assert_eq!(review.mode, RecordingReviewMode::Generated);
    }

    #[test]
    fn trim_and_edit_are_local_and_literal_baseline_is_immutable() {
        let mut review = RecordingReviewSession::new(result());
        review
            .toggle_suggestion(RecordingSuggestionId(1), false)
            .unwrap();
        let literal = review.literal_steps.clone();
        let middle = review.proposed_steps()[1].review_id;
        review.select(middle, false, false);
        review.trim_before_selection().unwrap();
        assert_eq!(review.proposed_steps().len(), 2);
        let id = review.proposed_steps()[0].review_id;
        let mut edited = review.step_for_editor(id).unwrap();
        edited.repeat = 4;
        review.replace_step(id, &edited).unwrap();
        assert_eq!(review.proposed_steps()[0].repeat, 4);
        assert_eq!(review.literal_steps, literal);
        assert_eq!(review.dropped_event_count, 4);
    }

    #[test]
    fn successful_uia_inspection_is_review_metadata_and_never_rewrites_the_click() {
        let mut recording = result();
        recording.plan = RecordingPlan {
            steps: vec![PlannedStep {
                action: MkAction::MouseClick(crate::mkmacro::MkMousePayload {
                    target: crate::mkmacro::MkCoordinateTarget::Screen {
                        point: crate::mkmacro::MkPoint { x: 4, y: 5 },
                    },
                    button: crate::mkmacro::MkMouseButton::Left,
                    clicks: 1,
                }),
                ..planned(1, 0)
            }],
        };
        recording.suggestions.clear();
        recording.literal_steps = vec![RecordedStep {
            timestamp_us: 10,
            delay_after_ms: 0,
            action: RecordedAction::Click {
                button: crate::mkmacro::MouseButton::Left,
                x: 4,
                y: 5,
                count: 1,
            },
            context: None,
        }];
        recording.click_inspections = vec![ClickInspection {
            timestamp_us: 10,
            info: UiElementInfo {
                selector: MkUiSelector {
                    automation_id: Some("save".into()),
                    name: Some("Save".into()),
                    control_type: Some(MkUiControlType::Button),
                    class_name: None,
                    framework_id: None,
                    ancestor_path: Vec::new(),
                },
                user_facing_name: "Save".into(),
                target_executable: "app.exe".into(),
                supported_patterns: Default::default(),
                bounds: None,
            },
        }];

        let review = RecordingReviewSession::new(recording);
        assert_eq!(review.click_inspections.len(), 1);
        assert_eq!(
            review.click_inspections[0]
                .info
                .selector
                .automation_id
                .as_deref(),
            Some("save")
        );
        assert!(matches!(
            review.proposed_steps()[0].action,
            MkAction::MouseClick(_)
        ));
        assert!(!review.proposed_steps().iter().any(|step| matches!(
            step.action,
            MkAction::UiInvoke(_)
                | MkAction::UiSetValue { .. }
                | MkAction::UiReadValue { .. }
                | MkAction::UiToggle(_)
                | MkAction::UiSelect(_)
                | MkAction::UiFocus(_)
                | MkAction::UiWait(_)
        )));
    }
}
