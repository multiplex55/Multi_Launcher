use super::{RuntimeOrigin, RuntimeRunMode};
use crate::mkmacro::{
    MkAction, MkCompiledProgram,
    executor::{DiagnosticKind, ExecutionEvent, ExecutionFrameContext, MonotonicClock},
};
use std::{
    collections::VecDeque,
    mem::size_of,
    sync::Arc,
    time::{Duration, SystemTime},
};

pub const MAX_COMPLETED_RUN_SUMMARIES: usize = 100;
pub const MAX_SUMMARY_METADATA_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_STEP_AGGREGATES_PER_RUN: usize = 1024;
const MAX_SUMMARY_NAME_BYTES: usize = 256;
const MAX_OPEN_OCCURRENCES: usize = 64;
const SUMMARY_FIXED_CHARGE: usize = 128;
const UNAVAILABLE_NAME: &str = "Macro name unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationSource {
    UnclassifiedApi,
    MacroDialog,
    LauncherCommand,
    Hotkey,
    HotFolder,
    RecordingPreview,
}

impl InvocationSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnclassifiedApi => "Unclassified API",
            Self::MacroDialog => "Macro dialog",
            Self::LauncherCommand => "Launcher command",
            Self::Hotkey => "Macro hotkey",
            Self::HotFolder => "Hot Folder",
            Self::RecordingPreview => "Recording preview",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummarySubset {
    Whole,
    FromStep { step_id: u64 },
    Selected { step_count: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryOutcome {
    Completed,
    Stopped,
    Failed(DiagnosticKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryActionCategory {
    ControlFlow,
    Wait,
    KeyboardMouse,
    TextEntry,
    Window,
    VirtualDesktop,
    Screen,
    Ocr,
    Launcher,
    DirectOpen,
    Clipboard,
    Prompt,
    NotificationSound,
    UiAutomation,
}

impl SummaryActionCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ControlFlow => "Control flow",
            Self::Wait => "Wait",
            Self::KeyboardMouse => "Keyboard or mouse",
            Self::TextEntry => "Text entry",
            Self::Window => "Window",
            Self::VirtualDesktop => "Virtual desktop",
            Self::Screen => "Screen or image",
            Self::Ocr => "OCR",
            Self::Launcher => "Launcher or process",
            Self::DirectOpen => "Direct open",
            Self::Clipboard => "Clipboard",
            Self::Prompt => "Prompt",
            Self::NotificationSound => "Notification or sound",
            Self::UiAutomation => "UI automation",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SummaryVisitTotals {
    /// Instruction visits that emitted StepStarted. Retries and repetitions
    /// inside one visit are intentionally not multiplied.
    pub started: u64,
    pub succeeded: u64,
    pub skipped: u64,
    pub failed: u64,
    pub interrupted: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryStepAggregate {
    pub macro_id: u64,
    pub step_id: u64,
    pub category: SummaryActionCategory,
    pub visits_started: u64,
    pub visits_succeeded: u64,
    pub visits_skipped: u64,
    pub visits_failed: u64,
    pub visits_interrupted: u64,
    /// Inclusive elapsed time; a caller visit includes time in nested calls.
    pub inclusive_elapsed: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeFailureLocation {
    pub kind: DiagnosticKind,
    pub macro_id: Option<u64>,
    pub step_id: Option<u64>,
    pub frame_id: Option<u64>,
    pub category: Option<SummaryActionCategory>,
}

impl SafeFailureLocation {
    pub const fn safe_reason(self) -> &'static str {
        match self.kind {
            DiagnosticKind::TargetNotFound => "Target was not found",
            DiagnosticKind::AmbiguousTarget => "Target was ambiguous",
            DiagnosticKind::Timeout => "Operation timed out",
            DiagnosticKind::UnsupportedOperation => "Operation is unsupported",
            DiagnosticKind::InputRejected => "Input was rejected",
            DiagnosticKind::InvalidTarget => "Target is invalid",
            DiagnosticKind::InvalidPlan => "Macro plan is invalid",
            DiagnosticKind::InvalidSelection => "Step selection is invalid",
            DiagnosticKind::Backend => "Backend operation failed",
            DiagnosticKind::Cancelled => "Execution was cancelled",
            DiagnosticKind::Panic => "Execution stopped unexpectedly",
            DiagnosticKind::RuntimeUnavailable => "Macro runtime is unavailable",
            DiagnosticKind::TypeMismatch => "A value had the wrong type",
            DiagnosticKind::InvalidRegex => "A pattern is invalid",
            DiagnosticKind::IterationLimit => "Execution reached a safety limit",
            DiagnosticKind::UnsupportedPattern => "Pattern is unsupported",
            DiagnosticKind::StaleElement => "A UI element changed",
            DiagnosticKind::ComFailure => "Windows automation failed",
        }
    }
}

/// One immutable, metadata-only terminal record. It deliberately has no
/// Clone implementation: history access lends records only while holding the
/// history lock so eviction cannot falsely free retained reader-owned copies.
#[derive(Debug, PartialEq, Eq)]
pub struct MacroRunSummary {
    pub run_id: u64,
    pub macro_id: u64,
    pub macro_name: String,
    pub macro_name_truncated: bool,
    pub invocation_source: InvocationSource,
    pub origin: RuntimeOrigin,
    pub mode: RuntimeRunMode,
    pub subset: SummarySubset,
    pub admitted_at: SystemTime,
    pub finished_at: SystemTime,
    pub elapsed: Duration,
    /// Present for worker-side compilation, including failed compilation.
    /// Pre-admission direct preparation is not relabeled as runtime work.
    pub worker_preparation: Option<Duration>,
    /// Offset from admission, absent when compilation or an owned start gate
    /// prevented execution from beginning.
    pub execution_started_after: Option<Duration>,
    pub paused: Duration,
    pub visits: SummaryVisitTotals,
    pub step_aggregates: Vec<SummaryStepAggregate>,
    /// Number of visits whose per-step details could not be retained.
    pub omitted_visit_details: u64,
    pub detail_truncated: bool,
    pub counter_overflowed: bool,
    pub outcome: SummaryOutcome,
    pub failure: Option<SafeFailureLocation>,
    charged_bytes: usize,
}

#[derive(Clone, Copy)]
struct SummaryLimits {
    completed_runs: usize,
    total_bytes: usize,
    step_rows_per_run: usize,
}

impl SummaryLimits {
    const fn production() -> Self {
        Self {
            completed_runs: MAX_COMPLETED_RUN_SUMMARIES,
            total_bytes: MAX_SUMMARY_METADATA_BYTES,
            step_rows_per_run: MAX_STEP_AGGREGATES_PER_RUN,
        }
    }
}

pub(crate) struct SummaryHistory {
    completed: VecDeque<MacroRunSummary>,
    revision: u64,
    used_bytes: usize,
    active_bytes: usize,
    limits: SummaryLimits,
}

impl SummaryHistory {
    pub(crate) fn new() -> Self {
        Self::with_limits(SummaryLimits::production())
    }

    fn with_limits(limits: SummaryLimits) -> Self {
        let completed = VecDeque::with_capacity(limits.completed_runs);
        let queue_charge = completed
            .capacity()
            .saturating_mul(size_of::<MacroRunSummary>());
        Self {
            completed,
            revision: 0,
            used_bytes: queue_charge,
            active_bytes: 0,
            limits,
        }
    }

    pub(crate) fn begin(
        &mut self,
        run_id: u64,
        macro_id: u64,
        macro_name: Option<&str>,
        invocation_source: InvocationSource,
        origin: RuntimeOrigin,
        mode: RuntimeRunMode,
        subset: SummarySubset,
        clock: Arc<dyn MonotonicClock>,
    ) -> Option<SummaryAccumulator> {
        if self.active_bytes != 0 {
            return None;
        }
        let source_name = macro_name.unwrap_or(UNAVAILABLE_NAME);
        let bounded_len = utf8_prefix_len(source_name, MAX_SUMMARY_NAME_BYTES);
        let base_charge = size_of::<SummaryAccumulator>()
            .checked_add(size_of::<MacroRunSummary>())?
            .checked_add(SUMMARY_FIXED_CHARGE)?;
        let mut reserved_charge = base_charge.checked_add(bounded_len)?;
        if !self.reserve_active(reserved_charge) {
            return None;
        }

        let mut name = String::new();
        let name_retained = name.try_reserve_exact(bounded_len).is_ok();
        if name_retained {
            name.push_str(&source_name[..bounded_len]);
        } else {
            self.release_active(reserved_charge);
            reserved_charge = base_charge;
            if !self.reserve_active(reserved_charge) {
                return None;
            }
        }
        let actual_name_charge = if name_retained { name.capacity() } else { 0 };
        let Some(actual_charge) = base_charge.checked_add(actual_name_charge) else {
            self.release_active(reserved_charge);
            return None;
        };
        if actual_charge < reserved_charge {
            self.release_active(reserved_charge - actual_charge);
        } else if actual_charge > reserved_charge
            && !self.reserve_active(actual_charge - reserved_charge)
        {
            self.release_active(reserved_charge);
            return None;
        }

        let now = clock.monotonic_now();
        let admitted_at = clock.wall_now();
        Some(SummaryAccumulator {
            run_id,
            macro_id,
            macro_name: name,
            macro_name_truncated: bounded_len < source_name.len() || !name_retained,
            invocation_source,
            origin,
            mode,
            subset,
            clock,
            admitted_at,
            admitted_tick: now,
            worker_preparation_started: None,
            worker_preparation: None,
            execution_started_after: None,
            visits: SummaryVisitTotals::default(),
            step_aggregates: Vec::new(),
            open: [None; MAX_OPEN_OCCURRENCES],
            omitted_visit_details: 0,
            detail_truncated: bounded_len < source_name.len() || !name_retained,
            counter_overflowed: false,
            failure: None,
            charged_bytes: actual_charge,
        })
    }

    fn reserve_active(&mut self, bytes: usize) -> bool {
        while self
            .used_bytes
            .checked_add(bytes)
            .is_none_or(|required| required > self.limits.total_bytes)
            && !self.completed.is_empty()
        {
            self.evict_oldest();
        }
        let Some(next) = self.used_bytes.checked_add(bytes) else {
            return false;
        };
        if next > self.limits.total_bytes {
            return false;
        }
        self.used_bytes = next;
        self.active_bytes = self.active_bytes.saturating_add(bytes);
        true
    }

    fn release_active(&mut self, bytes: usize) {
        let released = bytes.min(self.active_bytes);
        self.active_bytes -= released;
        self.used_bytes = self.used_bytes.saturating_sub(released);
    }

    pub(crate) fn abort(&mut self, accumulator: SummaryAccumulator) {
        self.release_active(accumulator.charged_bytes);
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest) = self.completed.pop_front() {
            self.used_bytes = self.used_bytes.saturating_sub(oldest.charged_bytes);
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub(crate) fn publish(
        &mut self,
        accumulator: SummaryAccumulator,
        outcome: SummaryOutcome,
        failure: Option<SafeFailureLocation>,
        paused: Duration,
    ) {
        let summary = accumulator.finish(outcome, failure, paused);
        self.active_bytes = 0;
        while self.completed.len() >= self.limits.completed_runs {
            self.evict_oldest();
        }
        self.completed.push_back(summary);
        self.revision = self.revision.saturating_add(1);
    }

    pub(crate) fn with_completed<R>(
        &self,
        inspect: impl FnOnce(u64, &VecDeque<MacroRunSummary>) -> R,
    ) -> R {
        inspect(self.revision, &self.completed)
    }

    #[cfg(test)]
    fn used_bytes(&self) -> usize {
        self.used_bytes
    }
}

pub(crate) struct SummaryAccumulator {
    run_id: u64,
    macro_id: u64,
    macro_name: String,
    macro_name_truncated: bool,
    invocation_source: InvocationSource,
    origin: RuntimeOrigin,
    mode: RuntimeRunMode,
    subset: SummarySubset,
    clock: Arc<dyn MonotonicClock>,
    admitted_at: SystemTime,
    admitted_tick: Duration,
    worker_preparation_started: Option<Duration>,
    worker_preparation: Option<Duration>,
    execution_started_after: Option<Duration>,
    visits: SummaryVisitTotals,
    step_aggregates: Vec<SummaryStepAggregate>,
    open: [Option<OpenOccurrence>; MAX_OPEN_OCCURRENCES],
    omitted_visit_details: u64,
    detail_truncated: bool,
    counter_overflowed: bool,
    failure: Option<SafeFailureLocation>,
    charged_bytes: usize,
}

#[derive(Clone, Copy)]
struct OpenOccurrence {
    frame_id: u64,
    step_id: u64,
    started_at: Duration,
    aggregate_index: Option<usize>,
}

impl SummaryAccumulator {
    pub(crate) fn mark_worker_preparation_started(&mut self) {
        self.worker_preparation_started = Some(self.clock.monotonic_now());
    }

    pub(crate) fn mark_execution_started(&mut self) {
        let now = self.clock.monotonic_now();
        self.execution_started_after = Some(now.saturating_sub(self.admitted_tick));
        if let Some(started) = self.worker_preparation_started.take() {
            self.worker_preparation = Some(now.saturating_sub(started));
        }
    }

    pub(crate) fn observe(
        &mut self,
        context: ExecutionFrameContext,
        event: &ExecutionEvent,
        program: &MkCompiledProgram,
        history: &mut SummaryHistory,
    ) {
        let now = self.clock.monotonic_now();
        match event {
            ExecutionEvent::StepStarted(step_id) => {
                bump(&mut self.visits.started, &mut self.counter_overflowed);
                let category = action_category(program, context.macro_id, *step_id);
                let aggregate_index =
                    self.ensure_aggregate(context.macro_id, *step_id, category, history);
                if let Some(index) = aggregate_index {
                    bump(
                        &mut self.step_aggregates[index].visits_started,
                        &mut self.counter_overflowed,
                    );
                }
                if let Some(slot) = self.open.iter_mut().find(|slot| slot.is_none()) {
                    *slot = Some(OpenOccurrence {
                        frame_id: context.frame_id,
                        step_id: *step_id,
                        started_at: now,
                        aggregate_index,
                    });
                } else {
                    self.detail_truncated = true;
                    bump(
                        &mut self.omitted_visit_details,
                        &mut self.counter_overflowed,
                    );
                }
            }
            ExecutionEvent::StepFinished(step_id) => {
                bump(&mut self.visits.succeeded, &mut self.counter_overflowed);
                self.close_occurrence(context, *step_id, now, VisitTerminal::Succeeded);
            }
            ExecutionEvent::StepFailed(step_id, diagnostic) => {
                bump(&mut self.visits.failed, &mut self.counter_overflowed);
                let category = action_category(program, context.macro_id, *step_id);
                self.failure = Some(SafeFailureLocation {
                    kind: diagnostic.kind,
                    macro_id: Some(context.macro_id),
                    step_id: Some(*step_id),
                    frame_id: Some(context.frame_id),
                    category: Some(category),
                });
                self.close_occurrence(context, *step_id, now, VisitTerminal::Failed);
            }
            ExecutionEvent::StepSkipped(step_id) => {
                bump(&mut self.visits.skipped, &mut self.counter_overflowed);
                let category = action_category(program, context.macro_id, *step_id);
                if let Some(index) =
                    self.ensure_aggregate(context.macro_id, *step_id, category, history)
                {
                    bump(
                        &mut self.step_aggregates[index].visits_skipped,
                        &mut self.counter_overflowed,
                    );
                } else {
                    self.mark_omitted_visit();
                }
            }
            ExecutionEvent::FrameExited { .. } => {
                self.interrupt_frame(context.frame_id, now);
            }
            _ => {}
        }
    }

    fn ensure_aggregate(
        &mut self,
        macro_id: u64,
        step_id: u64,
        category: SummaryActionCategory,
        history: &mut SummaryHistory,
    ) -> Option<usize> {
        if let Some(index) = self
            .step_aggregates
            .iter()
            .position(|row| row.macro_id == macro_id && row.step_id == step_id)
        {
            return Some(index);
        }
        let row_limit = MAX_STEP_AGGREGATES_PER_RUN.min(history.limits.step_rows_per_run);
        if self.step_aggregates.len() >= row_limit {
            self.detail_truncated = true;
            return None;
        }
        let old_capacity = self.step_aggregates.capacity();
        let target_capacity = if old_capacity == 0 {
            4.min(row_limit)
        } else {
            old_capacity.saturating_mul(2).min(row_limit)
        };
        let additional = target_capacity.saturating_sub(old_capacity);
        let row_charge = additional
            .checked_mul(size_of::<SummaryStepAggregate>())?
            .checked_mul(2)?;
        if !history.reserve_active(row_charge) {
            self.detail_truncated = true;
            return None;
        }
        self.charged_bytes = match self.charged_bytes.checked_add(row_charge) {
            Some(bytes) => bytes,
            None => {
                history.release_active(row_charge);
                self.counter_overflowed = true;
                self.detail_truncated = true;
                return None;
            }
        };
        if self
            .step_aggregates
            .try_reserve_exact(target_capacity.saturating_sub(self.step_aggregates.len()))
            .is_err()
        {
            self.charged_bytes = self.charged_bytes.saturating_sub(row_charge);
            history.release_active(row_charge);
            self.detail_truncated = true;
            return None;
        }
        let index = self.step_aggregates.len();
        self.step_aggregates.push(SummaryStepAggregate {
            macro_id,
            step_id,
            category,
            visits_started: 0,
            visits_succeeded: 0,
            visits_skipped: 0,
            visits_failed: 0,
            visits_interrupted: 0,
            inclusive_elapsed: Duration::ZERO,
        });
        Some(index)
    }

    fn close_occurrence(
        &mut self,
        context: ExecutionFrameContext,
        step_id: u64,
        now: Duration,
        terminal: VisitTerminal,
    ) {
        let occurrence = self.open.iter_mut().find_map(|slot| {
            slot.filter(|open| open.frame_id == context.frame_id && open.step_id == step_id)
                .map(|open| {
                    *slot = None;
                    open
                })
        });
        let Some(occurrence) = occurrence else {
            self.mark_omitted_visit();
            return;
        };
        if let Some(index) = occurrence.aggregate_index {
            let row = &mut self.step_aggregates[index];
            let duration = now.saturating_sub(occurrence.started_at);
            row.inclusive_elapsed = checked_duration_add(
                row.inclusive_elapsed,
                duration,
                &mut self.counter_overflowed,
            );
            match terminal {
                VisitTerminal::Succeeded => {
                    bump(&mut row.visits_succeeded, &mut self.counter_overflowed)
                }
                VisitTerminal::Failed => bump(&mut row.visits_failed, &mut self.counter_overflowed),
                VisitTerminal::Interrupted => {
                    bump(&mut row.visits_interrupted, &mut self.counter_overflowed)
                }
            }
        } else {
            self.mark_omitted_visit();
        }
    }

    fn interrupt_frame(&mut self, frame_id: u64, now: Duration) {
        for index in 0..self.open.len() {
            if let Some(open) = self.open[index].filter(|open| open.frame_id == frame_id) {
                self.open[index] = None;
                self.close_detached(open, now, VisitTerminal::Interrupted);
            }
        }
    }

    fn interrupt_all(&mut self, now: Duration) {
        for index in 0..self.open.len() {
            if let Some(open) = self.open[index].take() {
                self.close_detached(open, now, VisitTerminal::Interrupted);
            }
        }
    }

    fn close_detached(
        &mut self,
        occurrence: OpenOccurrence,
        now: Duration,
        terminal: VisitTerminal,
    ) {
        if let Some(index) = occurrence.aggregate_index {
            let row = &mut self.step_aggregates[index];
            row.inclusive_elapsed = checked_duration_add(
                row.inclusive_elapsed,
                now.saturating_sub(occurrence.started_at),
                &mut self.counter_overflowed,
            );
            if matches!(terminal, VisitTerminal::Interrupted) {
                bump(&mut row.visits_interrupted, &mut self.counter_overflowed);
            }
        }
        bump(&mut self.visits.interrupted, &mut self.counter_overflowed);
        self.mark_omitted_visit_if_unretained(occurrence.aggregate_index);
    }

    fn mark_omitted_visit(&mut self) {
        self.detail_truncated = true;
        bump(
            &mut self.omitted_visit_details,
            &mut self.counter_overflowed,
        );
    }

    fn mark_omitted_visit_if_unretained(&mut self, index: Option<usize>) {
        if index.is_none() {
            self.mark_omitted_visit();
        }
    }

    fn finish(
        mut self,
        outcome: SummaryOutcome,
        failure: Option<SafeFailureLocation>,
        paused: Duration,
    ) -> MacroRunSummary {
        let now = self.clock.monotonic_now();
        self.interrupt_all(now);
        if let SummaryOutcome::Failed(kind) = outcome {
            if let Some(mut location) = failure.or(self.failure) {
                location.kind = kind;
                self.failure = Some(location);
            } else {
                self.failure = Some(SafeFailureLocation {
                    kind,
                    macro_id: Some(self.macro_id),
                    step_id: None,
                    frame_id: None,
                    category: None,
                });
            }
        } else {
            self.failure = None;
        }
        if self.worker_preparation.is_none()
            && let Some(started) = self.worker_preparation_started.take()
        {
            self.worker_preparation = Some(now.saturating_sub(started));
        }
        MacroRunSummary {
            run_id: self.run_id,
            macro_id: self.macro_id,
            macro_name: self.macro_name,
            macro_name_truncated: self.macro_name_truncated,
            invocation_source: self.invocation_source,
            origin: self.origin,
            mode: self.mode,
            subset: self.subset,
            admitted_at: self.admitted_at,
            finished_at: self.clock.wall_now(),
            elapsed: now.saturating_sub(self.admitted_tick),
            worker_preparation: self.worker_preparation,
            execution_started_after: self.execution_started_after,
            paused,
            visits: self.visits,
            step_aggregates: self.step_aggregates,
            omitted_visit_details: self.omitted_visit_details,
            detail_truncated: self.detail_truncated,
            counter_overflowed: self.counter_overflowed,
            outcome,
            failure: self.failure,
            charged_bytes: self.charged_bytes,
        }
    }
}

#[derive(Clone, Copy)]
enum VisitTerminal {
    Succeeded,
    Failed,
    Interrupted,
}

fn bump(value: &mut u64, overflowed: &mut bool) {
    if let Some(next) = value.checked_add(1) {
        *value = next;
    } else {
        *overflowed = true;
    }
}

fn checked_duration_add(left: Duration, right: Duration, overflowed: &mut bool) -> Duration {
    match left.checked_add(right) {
        Some(value) => value,
        None => {
            *overflowed = true;
            Duration::MAX
        }
    }
}

fn utf8_prefix_len(value: &str, max_bytes: usize) -> usize {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn action_category(
    program: &MkCompiledProgram,
    macro_id: u64,
    step_id: u64,
) -> SummaryActionCategory {
    let action = program.plan(macro_id).and_then(|plan| {
        plan.step_to_instruction
            .get(&step_id)
            .map(|index| &plan.instructions[*index].step.action)
    });
    match action {
        Some(
            MkAction::Delay(_) | MkAction::WaitUntil { .. } | MkAction::WaitForVisualChange(_),
        ) => SummaryActionCategory::Wait,
        Some(
            MkAction::KeyDown(_)
            | MkAction::KeyUp(_)
            | MkAction::KeyPress(_)
            | MkAction::Hotkey(_)
            | MkAction::MouseMove(_)
            | MkAction::MouseDrag(_)
            | MkAction::MouseClick(_)
            | MkAction::ClickWithinRegion(_)
            | MkAction::MouseDown(_)
            | MkAction::MouseUp(_)
            | MkAction::MouseScroll { .. },
        ) => SummaryActionCategory::KeyboardMouse,
        Some(MkAction::Text(_)) => SummaryActionCategory::TextEntry,
        Some(
            MkAction::WindowActivate(_)
            | MkAction::WindowClose(_)
            | MkAction::WindowWait(_)
            | MkAction::WindowMoveResize(_)
            | MkAction::WindowState { .. },
        ) => SummaryActionCategory::Window,
        Some(MkAction::VirtualDesktop(_)) => SummaryActionCategory::VirtualDesktop,
        Some(
            MkAction::ImageFind(_)
            | MkAction::ImageClick(_)
            | MkAction::FindPixel(_)
            | MkAction::CaptureScreenshot(_)
            | MkAction::PixelCheck { .. },
        ) => SummaryActionCategory::Screen,
        Some(MkAction::OcrFindText(_) | MkAction::OcrClickText(_) | MkAction::OcrReadText(_)) => {
            SummaryActionCategory::Ocr
        }
        Some(MkAction::Process(_) | MkAction::LauncherCommand(_)) => {
            SummaryActionCategory::Launcher
        }
        Some(MkAction::OpenFolder(_) | MkAction::OpenUrl(_)) => SummaryActionCategory::DirectOpen,
        Some(MkAction::ReadClipboardText { .. } | MkAction::WriteClipboardText(_)) => {
            SummaryActionCategory::Clipboard
        }
        Some(MkAction::PromptInput(_)) => SummaryActionCategory::Prompt,
        Some(MkAction::Notify(_) | MkAction::PlaySound(_)) => {
            SummaryActionCategory::NotificationSound
        }
        Some(
            MkAction::UiInvoke(_)
            | MkAction::UiSetValue { .. }
            | MkAction::UiReadValue { .. }
            | MkAction::UiToggle(_)
            | MkAction::UiSelect(_)
            | MkAction::UiFocus(_)
            | MkAction::UiWait(_),
        ) => SummaryActionCategory::UiAutomation,
        _ => SummaryActionCategory::ControlFlow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        MkCallMacroPayload, MkDelayPayload, MkMacro, MkMacroDocument, MkOpenFolderPayload,
        MkOpenUrlPayload, MkStep, MkTextMode, MkTextPayload, compiler::compile_program,
        executor::ExecutionDiagnostic,
    };
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::UNIX_EPOCH,
    };

    #[derive(Default)]
    struct ManualClock {
        ticks_nanos: AtomicU64,
        wall_seconds: AtomicU64,
    }

    impl ManualClock {
        fn set_millis(&self, value: u64) {
            self.ticks_nanos
                .store(value.saturating_mul(1_000_000), Ordering::SeqCst);
        }

        fn set_wall_seconds(&self, value: u64) {
            self.wall_seconds.store(value, Ordering::SeqCst);
        }
    }

    impl MonotonicClock for ManualClock {
        fn monotonic_now(&self) -> Duration {
            Duration::from_nanos(self.ticks_nanos.load(Ordering::SeqCst))
        }

        fn wall_now(&self) -> SystemTime {
            UNIX_EPOCH + Duration::from_secs(self.wall_seconds.load(Ordering::SeqCst))
        }
    }

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        }
    }

    fn macro_(id: u64, steps: Vec<MkStep>) -> MkMacro {
        MkMacro {
            signature: Default::default(),
            id,
            name: format!("macro {id}"),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps,
        }
    }

    fn text(value: &str) -> MkAction {
        MkAction::Text(MkTextPayload {
            text: value.into(),
            mode: MkTextMode::Type,
        })
    }

    fn fixture_program() -> MkCompiledProgram {
        let root = macro_(
            1,
            vec![
                step(
                    1,
                    MkAction::CallMacro(MkCallMacroPayload {
                        macro_id: 2,
                        ..Default::default()
                    }),
                ),
                step(2, text("root repeat")),
                step(3, MkAction::Delay(MkDelayPayload::default())),
            ],
        );
        let child = macro_(2, vec![step(1, text("child"))]);
        compile_program(
            &MkMacroDocument {
                macros: vec![root, child],
                ..Default::default()
            },
            1,
        )
        .unwrap()
    }

    #[test]
    fn direct_open_steps_have_a_safe_distinct_summary_category() {
        let program = compile_program(
            &MkMacroDocument {
                macros: vec![macro_(
                    1,
                    vec![
                        step(
                            1,
                            MkAction::OpenFolder(MkOpenFolderPayload {
                                path: "C:\\private summary path".into(),
                            }),
                        ),
                        step(
                            2,
                            MkAction::OpenUrl(MkOpenUrlPayload {
                                url: "https://example.test/?token=private_summary_url".into(),
                            }),
                        ),
                    ],
                )],
                ..Default::default()
            },
            1,
        )
        .unwrap();

        assert_eq!(
            action_category(&program, 1, 1),
            SummaryActionCategory::DirectOpen
        );
        assert_eq!(
            action_category(&program, 1, 2),
            SummaryActionCategory::DirectOpen
        );
        assert_eq!(SummaryActionCategory::DirectOpen.label(), "Direct open");
    }

    #[test]
    fn nested_and_repeated_visits_keep_inclusive_monotonic_timings() {
        let clock = Arc::new(ManualClock::default());
        clock.set_wall_seconds(100);
        let mut history = SummaryHistory::with_limits(SummaryLimits {
            completed_runs: 2,
            total_bytes: 64 * 1024,
            step_rows_per_run: 8,
        });
        let mut accumulator = history
            .begin(
                7,
                1,
                Some("root"),
                InvocationSource::Hotkey,
                RuntimeOrigin::Stored,
                RuntimeRunMode::Debug,
                SummarySubset::FromStep { step_id: 1 },
                clock.clone(),
            )
            .unwrap();
        accumulator.mark_worker_preparation_started();
        clock.set_millis(4);
        accumulator.mark_execution_started();

        let program = fixture_program();
        let root = ExecutionFrameContext::root(1);
        let child = ExecutionFrameContext {
            frame_id: 2,
            macro_id: 2,
            caller_step_id: Some(1),
            depth: 2,
        };
        accumulator.observe(
            root,
            &ExecutionEvent::StepStarted(1),
            &program,
            &mut history,
        );
        clock.set_millis(8);
        accumulator.observe(
            child,
            &ExecutionEvent::StepStarted(1),
            &program,
            &mut history,
        );
        clock.set_millis(11);
        accumulator.observe(
            child,
            &ExecutionEvent::StepFinished(1),
            &program,
            &mut history,
        );
        clock.set_millis(13);
        accumulator.observe(
            child,
            &ExecutionEvent::FrameExited {
                caller_boundary: None,
            },
            &program,
            &mut history,
        );
        clock.set_millis(15);
        accumulator.observe(
            root,
            &ExecutionEvent::StepFinished(1),
            &program,
            &mut history,
        );
        accumulator.observe(
            root,
            &ExecutionEvent::StepStarted(2),
            &program,
            &mut history,
        );
        clock.set_millis(17);
        accumulator.observe(
            root,
            &ExecutionEvent::StepFinished(2),
            &program,
            &mut history,
        );
        clock.set_millis(20);
        accumulator.observe(
            root,
            &ExecutionEvent::StepStarted(2),
            &program,
            &mut history,
        );
        clock.set_millis(24);
        accumulator.observe(
            root,
            &ExecutionEvent::StepFinished(2),
            &program,
            &mut history,
        );
        accumulator.observe(
            root,
            &ExecutionEvent::StepSkipped(3),
            &program,
            &mut history,
        );

        // Wall time may move backwards; elapsed time remains monotonic.
        clock.set_wall_seconds(50);
        history.publish(accumulator, SummaryOutcome::Completed, None, Duration::ZERO);
        history.with_completed(|revision, records| {
            assert_eq!(revision, 1);
            let summary = records.front().unwrap();
            assert_eq!(summary.run_id, 7);
            assert_eq!(summary.invocation_source, InvocationSource::Hotkey);
            assert_eq!(summary.subset, SummarySubset::FromStep { step_id: 1 });
            assert_eq!(summary.worker_preparation, Some(Duration::from_millis(4)));
            assert_eq!(
                summary.execution_started_after,
                Some(Duration::from_millis(4))
            );
            assert_eq!(summary.elapsed, Duration::from_millis(24));
            assert!(summary.finished_at < summary.admitted_at);
            assert_eq!(summary.visits.started, 4);
            assert_eq!(summary.visits.succeeded, 4);
            assert_eq!(summary.visits.skipped, 1);

            let root_call = summary
                .step_aggregates
                .iter()
                .find(|row| row.macro_id == 1 && row.step_id == 1)
                .unwrap();
            assert_eq!(root_call.inclusive_elapsed, Duration::from_millis(11));
            assert_eq!(root_call.category, SummaryActionCategory::ControlFlow);

            let child_text = summary
                .step_aggregates
                .iter()
                .find(|row| row.macro_id == 2 && row.step_id == 1)
                .unwrap();
            assert_eq!(child_text.inclusive_elapsed, Duration::from_millis(3));

            let repeated_text = summary
                .step_aggregates
                .iter()
                .find(|row| row.macro_id == 1 && row.step_id == 2)
                .unwrap();
            assert_eq!(repeated_text.visits_started, 2);
            assert_eq!(repeated_text.visits_succeeded, 2);
            assert_eq!(repeated_text.inclusive_elapsed, Duration::from_millis(6));

            let skipped_wait = summary
                .step_aggregates
                .iter()
                .find(|row| row.macro_id == 1 && row.step_id == 3)
                .unwrap();
            assert_eq!(skipped_wait.visits_skipped, 1);
            assert_eq!(skipped_wait.category, SummaryActionCategory::Wait);
        });
    }

    #[test]
    fn stopped_run_closes_active_visit_as_interrupted() {
        let clock = Arc::new(ManualClock::default());
        let mut history = SummaryHistory::with_limits(SummaryLimits {
            completed_runs: 1,
            total_bytes: 32 * 1024,
            step_rows_per_run: 1,
        });
        let program = fixture_program();
        let mut accumulator = history
            .begin(
                8,
                1,
                Some("root"),
                InvocationSource::HotFolder,
                RuntimeOrigin::Stored,
                RuntimeRunMode::Normal,
                SummarySubset::Whole,
                clock.clone(),
            )
            .unwrap();
        let root = ExecutionFrameContext::root(1);
        accumulator.observe(
            root,
            &ExecutionEvent::StepStarted(1),
            &program,
            &mut history,
        );
        clock.set_millis(6);
        history.publish(accumulator, SummaryOutcome::Stopped, None, Duration::ZERO);
        history.with_completed(|_, records| {
            let summary = records.front().unwrap();
            assert_eq!(summary.outcome, SummaryOutcome::Stopped);
            assert_eq!(summary.visits.started, 1);
            assert_eq!(summary.visits.interrupted, 1);
            let visit = &summary.step_aggregates[0];
            assert_eq!(visit.visits_interrupted, 1);
            assert_eq!(visit.inclusive_elapsed, Duration::from_millis(6));
        });
    }

    #[test]
    fn row_and_history_caps_preserve_totals_and_evict_oldest_completed() {
        let clock = Arc::new(ManualClock::default());
        let mut history = SummaryHistory::with_limits(SummaryLimits {
            completed_runs: 2,
            total_bytes: 64 * 1024,
            step_rows_per_run: 1,
        });
        let program = fixture_program();
        let root = ExecutionFrameContext::root(1);
        let mut first = history
            .begin(
                1,
                1,
                Some("root"),
                InvocationSource::MacroDialog,
                RuntimeOrigin::Stored,
                RuntimeRunMode::Normal,
                SummarySubset::Whole,
                clock.clone(),
            )
            .unwrap();
        assert!(
            history
                .begin(
                    99,
                    1,
                    Some("overlap"),
                    InvocationSource::UnclassifiedApi,
                    RuntimeOrigin::Stored,
                    RuntimeRunMode::Normal,
                    SummarySubset::Whole,
                    clock.clone(),
                )
                .is_none(),
            "only the active root run may own an accumulator"
        );
        first.observe(
            root,
            &ExecutionEvent::StepStarted(1),
            &program,
            &mut history,
        );
        clock.set_millis(1);
        first.observe(
            root,
            &ExecutionEvent::StepFinished(1),
            &program,
            &mut history,
        );
        first.observe(
            root,
            &ExecutionEvent::StepStarted(2),
            &program,
            &mut history,
        );
        clock.set_millis(2);
        first.observe(
            root,
            &ExecutionEvent::StepFinished(2),
            &program,
            &mut history,
        );
        history.publish(first, SummaryOutcome::Completed, None, Duration::ZERO);
        history.with_completed(|_, records| {
            let summary = records.front().unwrap();
            assert_eq!(summary.visits.started, 2);
            assert_eq!(summary.visits.succeeded, 2);
            assert_eq!(summary.step_aggregates.len(), 1);
            assert!(summary.detail_truncated);
            assert_eq!(summary.omitted_visit_details, 1);
        });

        for run_id in [2, 3] {
            let long_name = "é".repeat(200);
            let name = if run_id == 3 {
                long_name.as_str()
            } else {
                "root"
            };
            let record = history
                .begin(
                    run_id,
                    1,
                    Some(name),
                    InvocationSource::LauncherCommand,
                    RuntimeOrigin::Stored,
                    RuntimeRunMode::Normal,
                    SummarySubset::Whole,
                    clock.clone(),
                )
                .unwrap();
            history.publish(record, SummaryOutcome::Completed, None, Duration::ZERO);
            assert!(history.used_bytes() <= history.limits.total_bytes);
        }
        history.with_completed(|revision, records| {
            assert!(
                revision >= 4,
                "eviction and publication both advance revision"
            );
            assert_eq!(records.len(), 2);
            assert_eq!(records[0].run_id, 2);
            assert_eq!(records[1].run_id, 3);
            assert_eq!(records[1].macro_name.len(), MAX_SUMMARY_NAME_BYTES);
            assert!(records[1].macro_name_truncated);
            assert_eq!(records[1].macro_name, "é".repeat(128));
        });
    }

    #[test]
    fn active_byte_pressure_evicts_old_rows_before_growth_and_keeps_current_terminal() {
        let clock = Arc::new(ManualClock::default());
        let mut history = SummaryHistory::with_limits(SummaryLimits {
            completed_runs: 2,
            total_bytes: usize::MAX,
            step_rows_per_run: 1,
        });
        let queue_charge = history.used_bytes;
        let first = history
            .begin(
                1,
                1,
                Some(""),
                InvocationSource::MacroDialog,
                RuntimeOrigin::Stored,
                RuntimeRunMode::Normal,
                SummarySubset::Whole,
                clock.clone(),
            )
            .unwrap();
        let base_charge = first.charged_bytes;
        history.publish(first, SummaryOutcome::Completed, None, Duration::ZERO);

        let row_charge = 2 * size_of::<SummaryStepAggregate>();
        assert!(base_charge > row_charge - 1);
        history.limits.total_bytes = queue_charge + base_charge + row_charge - 1;

        let mut current = history
            .begin(
                2,
                1,
                Some(""),
                InvocationSource::MacroDialog,
                RuntimeOrigin::Stored,
                RuntimeRunMode::Normal,
                SummarySubset::Whole,
                clock.clone(),
            )
            .unwrap();
        assert!(
            history.completed.is_empty(),
            "byte pressure evicts the old row"
        );
        let before_growth = history.used_bytes;
        assert_eq!(before_growth, queue_charge + current.charged_bytes);

        current.visits.started = u64::MAX;
        let program = fixture_program();
        let root = ExecutionFrameContext::root(1);
        current.observe(
            root,
            &ExecutionEvent::StepStarted(1),
            &program,
            &mut history,
        );
        assert_eq!(
            history.used_bytes, before_growth,
            "failed pre-growth admission must not charge or allocate a row"
        );
        assert!(current.step_aggregates.is_empty());
        assert_eq!(current.step_aggregates.capacity(), 0);
        assert!(current.detail_truncated);
        assert!(current.counter_overflowed);
        assert_eq!(current.visits.started, u64::MAX);

        current.observe(
            root,
            &ExecutionEvent::StepFinished(1),
            &program,
            &mut history,
        );
        history.publish(current, SummaryOutcome::Completed, None, Duration::ZERO);
        assert!(history.used_bytes <= history.limits.total_bytes);
        history.with_completed(|_, records| {
            assert_eq!(records.len(), 1);
            let summary = &records[0];
            assert_eq!(summary.run_id, 2);
            assert_eq!(summary.visits.started, u64::MAX);
            assert_eq!(summary.visits.succeeded, 1);
            assert_eq!(summary.omitted_visit_details, 1);
            assert!(summary.detail_truncated);
            assert!(summary.counter_overflowed);
            assert!(summary.step_aggregates.is_empty());
        });
    }

    #[test]
    fn failure_summary_keeps_only_safe_location_not_diagnostic_payload() {
        let clock = Arc::new(ManualClock::default());
        let mut history = SummaryHistory::with_limits(SummaryLimits {
            completed_runs: 1,
            total_bytes: 32 * 1024,
            step_rows_per_run: 2,
        });
        let program = fixture_program();
        let mut accumulator = history
            .begin(
                4,
                1,
                Some("root"),
                InvocationSource::UnclassifiedApi,
                RuntimeOrigin::Stored,
                RuntimeRunMode::Normal,
                SummarySubset::Whole,
                clock.clone(),
            )
            .unwrap();
        let context = ExecutionFrameContext::root(1);
        accumulator.observe(
            context,
            &ExecutionEvent::StepStarted(2),
            &program,
            &mut history,
        );
        accumulator.observe(
            context,
            &ExecutionEvent::StepFailed(
                2,
                ExecutionDiagnostic::new(DiagnosticKind::Backend, "PRIVATE_SECRET_MESSAGE")
                    .context("private-context-key", "PRIVATE_SECRET_VALUE"),
            ),
            &program,
            &mut history,
        );
        history.publish(
            accumulator,
            SummaryOutcome::Failed(DiagnosticKind::Backend),
            None,
            Duration::ZERO,
        );
        history.with_completed(|_, records| {
            let summary = records.front().unwrap();
            assert_eq!(
                summary.outcome,
                SummaryOutcome::Failed(DiagnosticKind::Backend)
            );
            let failure = summary.failure.unwrap();
            assert_eq!(failure.step_id, Some(2));
            assert_eq!(failure.safe_reason(), "Backend operation failed");
            let debug = format!("{summary:?}");
            for secret in [
                "PRIVATE_SECRET_MESSAGE",
                "private-context-key",
                "PRIVATE_SECRET_VALUE",
            ] {
                assert!(!debug.contains(secret));
            }
        });
    }
}
