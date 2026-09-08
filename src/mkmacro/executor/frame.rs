//! Per-invocation continuation and root-owned execution resources. Effects keep
//! using Executor's existing backend boundary; a frame never owns input cleanup
//! or a separate RunControl.
use super::*;
use crate::mkmacro::{MkCompiledProgram, MkInvocationValues, invocation};

struct SafeBoundary {
    step_id: Option<u64>,
    variables: RuntimeVariables,
}

#[derive(Clone, Copy)]
struct AttemptState {
    repetition: u32,
    attempt: u32,
    attempts: u32,
}

enum InstructionPhase {
    Enter,
    Attempt(AttemptState),
    AwaitChild(AttemptState),
    Pace { repetition: u32 },
    Finish { error: Option<ExecutionDiagnostic> },
}

struct ExecutionFrame<'plan> {
    plan: &'plan MkExecutionPlan,
    macro_name: Arc<str>,
    context: ExecutionFrameContext,
    returned: Option<MkInvocationValues>,
    pc: usize,
    variables: RuntimeVariables,
    loops: HashMap<usize, u32>,
    phase: InstructionPhase,
    // Partial effects and failed condition evaluation must never overwrite the
    // last settled debugger boundary. Normal execution needs no snapshot clone.
    safe_boundary: Option<SafeBoundary>,
}

impl<'plan> ExecutionFrame<'plan> {
    #[cfg(test)]
    fn new(plan: &'plan MkExecutionPlan, mode: ExecutionMode) -> Self {
        Self::with_variables(
            plan,
            mode,
            RuntimeVariables::new(),
            ExecutionFrameContext {
                frame_id: 1,
                macro_id: plan.macro_id,
                caller_step_id: None,
                depth: 1,
            },
        )
    }

    fn with_variables(
        plan: &'plan MkExecutionPlan,
        mode: ExecutionMode,
        mut variables: RuntimeVariables,
        context: ExecutionFrameContext,
    ) -> Self {
        variables.insert("macro.id".into(), MkValue::Number(plan.macro_id as f64));
        variables.insert("macro.name".into(), MkValue::String(plan.name.clone()));
        variables.insert("last_action_success".into(), MkValue::Boolean(true));
        let safe_boundary = (mode == ExecutionMode::Debug).then(|| SafeBoundary {
            step_id: None,
            variables: variables.clone(),
        });
        Self {
            plan,
            macro_name: Arc::from(plan.name.as_str()),
            context,
            returned: None,
            pc: 0,
            variables,
            loops: HashMap::new(),
            phase: InstructionPhase::Enter,
            safe_boundary,
        }
    }

    fn snapshot(&self) -> ExecutionFrameSnapshot {
        ExecutionFrameSnapshot {
            context: self.context,
            macro_name: self.macro_name.clone(),
            active_step_id: None,
        }
    }

    fn begin_repetition(&mut self, repetition: u32) {
        let step = &self.plan.instructions[self.pc].step;
        let attempts = match (step.action.is_structural(), &step.on_error) {
            (true, _) => 1,
            (_, super::super::MkErrorPolicy::Retry(retry)) => retry.attempts.max(1),
            _ => 1,
        };
        self.variables
            .insert("iteration".into(), MkValue::Number(repetition as f64));
        self.phase = InstructionPhase::Attempt(AttemptState {
            repetition,
            attempt: 1,
            attempts,
        });
    }

    fn next_pc(&mut self, executor: &Executor) -> ExecResult<usize> {
        let instruction = &self.plan.instructions[self.pc];
        Ok(match (&instruction.step.action, &instruction.jump) {
            (MkAction::If(condition) | MkAction::WhileStart { condition }, Jump::IfFalse(to)) => {
                executor.control.checkpoint()?;
                if executor.condition(self.plan.macro_id, condition, &mut self.variables)? {
                    self.pc + 1
                } else {
                    *to
                }
            }
            (_, Jump::To(to) | Jump::Break(to) | Jump::Continue(to)) => *to,
            (MkAction::RepeatStart { count: 0 }, Jump::RepeatBegin { exit }) => *exit,
            (MkAction::RepeatStart { count }, _) => {
                self.loops.insert(self.pc, *count);
                self.pc + 1
            }
            (_, Jump::RepeatEnd { start, exit }) => {
                let entry = self.loops.entry(start.saturating_sub(1)).or_default();
                if *entry > 1 {
                    *entry -= 1;
                    *start
                } else {
                    self.loops.remove(&start.saturating_sub(1));
                    *exit
                }
            }
            (_, Jump::WhileEnd { condition }) => *condition,
            _ => self.pc + 1,
        })
    }
}

struct RootSession<'executor, 'observer> {
    // Rust drops struct fields in declaration order. Owned input must be
    // released while the run is still active, including on panic unwinding.
    input: InputCleanupGuard,
    _activity: RunActivityGuard<'executor>,
    // All effects, interruptible waits, random samplers and control are root
    // resources borrowed from the existing public Executor facade.
    executor: &'executor Executor,
    options: ExecutionOptions,
    observe: &'observer dyn Fn(ExecutionFrameContext, ExecutionEvent),
    transitions: u64,
    next_frame_id: u64,
}

impl<'executor, 'observer> RootSession<'executor, 'observer> {
    fn new(
        executor: &'executor Executor,
        options: ExecutionOptions,
        observe: &'observer dyn Fn(ExecutionFrameContext, ExecutionEvent),
    ) -> Self {
        let activity = RunActivityGuard(&executor.control);
        let input = InputCleanupGuard::new(executor.backends.input.clone());
        Self {
            input,
            _activity: activity,
            executor,
            options,
            observe,
            transitions: 0,
            next_frame_id: 2,
        }
    }

    fn emit_variables(
        &self,
        context: ExecutionFrameContext,
        boundary: &SafeBoundary,
        reason: DebugSnapshotReason,
    ) {
        (self.observe)(
            context,
            ExecutionEvent::DebugVariables {
                step_id: boundary.step_id,
                variables: boundary.variables.clone(),
                reason,
            },
        );
    }

    fn run_stack<'plan>(
        &mut self,
        root: ExecutionFrame<'plan>,
        program: Option<&'plan MkCompiledProgram>,
    ) -> ExecResult {
        let mut frames = vec![root];
        let mut pending_error = None;
        loop {
            let frame = frames.last_mut().expect("root remains until completion");
            let completion = if let Some(error) = pending_error.take() {
                Err(error)
            } else if frame.pc >= frame.plan.instructions.len() {
                self.executor.control.checkpoint().and_then(|_| {
                    frame.returned.take().map(Ok).unwrap_or_else(|| {
                        if frame.plan.signature.outputs().is_empty() {
                            Ok(MkInvocationValues::new())
                        } else {
                            Err(ExecutionDiagnostic::new(
                                DiagnosticKind::InvalidPlan,
                                "Macro declaring outputs reached its end without Return",
                            ))
                        }
                    })
                })
            } else {
                match self.advance(frame, program) {
                    Ok(Some(child)) => {
                        (self.observe)(
                            child.context,
                            ExecutionEvent::FrameEntered(child.snapshot()),
                        );
                        if let Some(boundary) = &child.safe_boundary {
                            self.emit_variables(
                                child.context,
                                boundary,
                                DebugSnapshotReason::RunStarted,
                            );
                        }
                        frames.push(child);
                        continue;
                    }
                    Ok(None) => continue,
                    Err(error) => Err(error),
                }
            };
            let completed = frames.pop().expect("completed frame exists");
            let reason = match &completion {
                Ok(_) => DebugSnapshotReason::RunFinished,
                Err(error) if error.kind == DiagnosticKind::Cancelled => {
                    DebugSnapshotReason::RunCancelled
                }
                Err(_) => DebugSnapshotReason::RunFailed,
            };
            if let Some(boundary) = &completed.safe_boundary {
                self.emit_variables(completed.context, boundary, reason);
            }
            let caller_boundary = frames
                .last()
                .and_then(|caller| caller.safe_boundary.as_ref())
                .map(|boundary| FrameVariableBoundary {
                    step_id: boundary.step_id,
                    variables: boundary.variables.clone(),
                });
            (self.observe)(
                completed.context,
                ExecutionEvent::FrameExited { caller_boundary },
            );
            let Some(caller) = frames.last_mut() else {
                return completion.map(|_| ());
            };
            let InstructionPhase::AwaitChild(attempt) = caller.phase else {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidPlan,
                    "Caller is not awaiting a child",
                ));
            };
            let MkAction::CallMacro(call) = &caller.plan.instructions[caller.pc].step.action else {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidPlan,
                    "Suspended instruction is not a Call",
                ));
            };
            let result = completion
                .and_then(|returned| {
                    self.executor
                        .control
                        .checkpoint()
                        .and_then(|_| {
                            invocation::prepare_output_writes(
                                &completed.plan.signature,
                                &call.outputs,
                                &returned,
                            )
                        })
                        .and_then(|writes| {
                            self.executor.control.checkpoint()?;
                            caller.variables.extend(writes);
                            Ok(())
                        })
                })
                .map_err(|error| {
                    error.context("callee_macro_id", completed.plan.macro_id.to_string())
                });
            if let Err(error) = self.complete_attempt(caller, attempt, result) {
                pending_error = Some(error);
            }
        }
    }

    #[cfg(test)]
    fn run_frame(&mut self, frame: &mut ExecutionFrame<'_>) -> ExecResult {
        while frame.pc < frame.plan.instructions.len() {
            assert!(self.advance(frame, None)?.is_none());
        }
        Ok(())
    }

    fn advance<'plan>(
        &mut self,
        frame: &mut ExecutionFrame<'plan>,
        program: Option<&'plan MkCompiledProgram>,
    ) -> ExecResult<Option<ExecutionFrame<'plan>>> {
        match std::mem::replace(&mut frame.phase, InstructionPhase::Enter) {
            InstructionPhase::Enter => self.enter_instruction(frame)?,
            InstructionPhase::Attempt(attempt) => {
                let step = &frame.plan.instructions[frame.pc].step;
                tracing::debug!(
                    macro_id = frame.plan.macro_id,
                    step_id = step.id,
                    attempt = attempt.attempt,
                    "executing macro step"
                );
                if matches!(step.action, MkAction::CallMacro(_) | MkAction::Return(_)) {
                    if let Err(error) = self.executor.control.checkpoint() {
                        self.complete_attempt(frame, attempt, Err(error))?;
                        return Ok(None);
                    }
                }
                let result = match &step.action {
                    MkAction::CallMacro(call) => match self.prepare_child(frame, call, program) {
                        Ok(child) => {
                            frame.phase = InstructionPhase::AwaitChild(attempt);
                            return Ok(Some(child));
                        }
                        Err(error) => Err(error),
                    },
                    MkAction::Return(payload) => {
                        match invocation::prepare_return(
                            &frame.plan.signature,
                            payload,
                            &frame.variables,
                        )
                        .and_then(|outputs| self.executor.control.checkpoint().map(|_| outputs))
                        {
                            Ok(outputs) => {
                                // Return exits immediately, without repetitions or pacing.
                                frame.returned = Some(outputs);
                                frame
                                    .variables
                                    .insert("last_action_success".into(), MkValue::Boolean(true));
                                frame.phase = InstructionPhase::Finish { error: None };
                                return Ok(None);
                            }
                            Err(error) => Err(error),
                        }
                    }
                    action => self.executor.action(
                        frame.plan.macro_id,
                        action,
                        &frame.plan.playback,
                        &mut frame.variables,
                        &mut self.input,
                    ),
                };
                self.complete_attempt(frame, attempt, result)?;
            }
            InstructionPhase::AwaitChild(_) => {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidPlan,
                    "Cannot advance a suspended Call",
                ));
            }
            InstructionPhase::Pace { repetition } => self.pace_repetition(frame, repetition)?,
            InstructionPhase::Finish { error } => self.finish_instruction(frame, error)?,
        }
        Ok(None)
    }

    fn prepare_child<'plan>(
        &mut self,
        caller: &ExecutionFrame<'_>,
        call: &crate::mkmacro::MkCallMacroPayload,
        program: Option<&'plan MkCompiledProgram>,
    ) -> ExecResult<ExecutionFrame<'plan>> {
        let plan = program
            .and_then(|program| program.plan(call.macro_id))
            .ok_or_else(|| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "Call target is unavailable in the compiled program",
                )
                .context("target_macro_id", call.macro_id.to_string())
            })?;
        if !plan.enabled {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Call target is disabled",
            )
            .context("target_macro_id", call.macro_id.to_string()));
        }
        const MAX_FRAMES: usize = 64;
        if caller.context.depth >= MAX_FRAMES {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::IterationLimit,
                "Macro call depth limit (64 frames) exceeded",
            )
            .context("limit", MAX_FRAMES.to_string()));
        }
        let variables = invocation::prepare_call(&plan.signature, call, &caller.variables)?;
        self.executor.control.checkpoint()?;
        let context = ExecutionFrameContext {
            frame_id: self.next_frame_id,
            macro_id: plan.macro_id,
            caller_step_id: Some(caller.plan.instructions[caller.pc].step.id),
            depth: caller.context.depth + 1,
        };
        self.next_frame_id += 1;
        Ok(ExecutionFrame::with_variables(
            plan,
            self.options.mode,
            variables,
            context,
        ))
    }

    fn enter_instruction(&mut self, frame: &mut ExecutionFrame<'_>) -> ExecResult {
        if self.executor.backends.input.escape_pressed() {
            self.executor.control.stop();
        }
        self.executor.control.checkpoint()?;
        self.transitions += 1;
        if self.transitions > Executor::MAX_CONTROL_TRANSITIONS {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::IterationLimit,
                "control-flow safety limit (100000 transitions) exceeded",
            )
            .context("limit", Executor::MAX_CONTROL_TRANSITIONS.to_string()));
        }
        let step = &frame.plan.instructions[frame.pc].step;
        if !step.enabled {
            (self.observe)(frame.context, ExecutionEvent::StepSkipped(step.id));
            frame.pc += 1;
            return Ok(());
        }
        frame
            .variables
            .insert("step.id".into(), MkValue::Number(step.id as f64));
        if self.options.mode == ExecutionMode::Debug && step.breakpoint {
            // The observer may synchronously Resume or Stop. Pause must be set
            // before publishing the breakpoint, never after observer return.
            self.executor.control.pause();
            (self.observe)(
                frame.context,
                ExecutionEvent::BreakpointHit {
                    step_id: step.id,
                    variables: frame.variables.clone(),
                },
            );
            self.executor.control.checkpoint()?;
        }
        (self.observe)(frame.context, ExecutionEvent::StepStarted(step.id));
        if step.repeat == 0 {
            // Retain the existing behavior for directly supplied, unvalidated
            // plans even though authored plans require a positive repeat count.
            frame.phase = InstructionPhase::Finish { error: None };
        } else {
            frame.begin_repetition(0);
        }
        Ok(())
    }

    /// One completion boundary for an action attempt. Its continuation stays in
    /// the frame so the execution engine does not rely on nested retry loops.
    fn complete_attempt(
        &mut self,
        frame: &mut ExecutionFrame<'_>,
        attempt: AttemptState,
        result: ExecResult,
    ) -> ExecResult {
        let step = &frame.plan.instructions[frame.pc].step;
        match result {
            Ok(()) => {
                frame
                    .variables
                    .insert("last_action_success".into(), MkValue::Boolean(true));
                frame.phase = InstructionPhase::Pace {
                    repetition: attempt.repetition,
                };
            }
            Err(error) => {
                let mut error = error;
                error
                    .context
                    .entry("origin_macro_id".into())
                    .or_insert_with(|| frame.plan.macro_id.to_string());
                error
                    .context
                    .entry("origin_step_id".into())
                    .or_insert_with(|| step.id.to_string());
                let error = error
                    .context("step", step.id.to_string())
                    .context("step_id", step.id.to_string())
                    .context("backend_operation", action_name(&step.action))
                    .context("attempt", attempt.attempt.to_string())
                    .context(
                        "attempts_exhausted",
                        (attempt.attempt == attempt.attempts).to_string(),
                    );
                frame
                    .variables
                    .insert("last_action_success".into(), MkValue::Boolean(false));
                tracing::warn!(macro_id = frame.plan.macro_id, step_id = step.id, attempt = attempt.attempt, error = %error, "macro step attempt failed");
                if error.kind != DiagnosticKind::Cancelled
                    && attempt.attempt < attempt.attempts
                    && let super::super::MkErrorPolicy::Retry(retry) = &step.on_error
                {
                    // Error-policy backoff is not scaled by playback speed.
                    self.executor.wait(Duration::from_millis(retry.delay_ms))?;
                    frame.phase = InstructionPhase::Attempt(AttemptState {
                        attempt: attempt.attempt + 1,
                        ..attempt
                    });
                } else {
                    frame.phase = InstructionPhase::Finish { error: Some(error) };
                }
            }
        }
        Ok(())
    }

    fn pace_repetition(&mut self, frame: &mut ExecutionFrame<'_>, repetition: u32) -> ExecResult {
        let step = &frame.plan.instructions[frame.pc].step;
        let normal =
            scale_playback_duration(step.delay_after_ms, frame.plan.playback.speed_percent);
        let delay = if step.action.is_structural() {
            normal
        } else {
            add_sampled_random_delay(normal, sample_delay(frame.plan.playback.random_delay_ms))
        };
        if delay > 0 {
            self.executor.wait(Duration::from_millis(delay))?;
        }
        if repetition + 1 < step.repeat {
            frame.begin_repetition(repetition + 1);
        } else {
            frame.phase = InstructionPhase::Finish { error: None };
        }
        Ok(())
    }

    fn finish_instruction(
        &mut self,
        frame: &mut ExecutionFrame<'_>,
        error: Option<ExecutionDiagnostic>,
    ) -> ExecResult {
        let step = &frame.plan.instructions[frame.pc].step;
        let step_id = step.id;
        if let Some(error) = error {
            (self.observe)(
                frame.context,
                ExecutionEvent::StepFailed(step_id, error.clone()),
            );
            if error.kind == DiagnosticKind::Cancelled
                || !matches!(step.on_error, super::super::MkErrorPolicy::Continue)
            {
                return Err(error);
            }
        } else {
            let outcome = StepOutcome::for_action(&step.action, &frame.variables);
            if outcome.last_image_found.is_some() {
                (self.observe)(frame.context, ExecutionEvent::StepOutcome(step_id, outcome));
            }
            (self.observe)(frame.context, ExecutionEvent::StepFinished(step_id));
        }
        // StepFinished historically precedes If/While evaluation, but a safe
        // snapshot is published only after that evaluation and jump succeed.
        let next_pc = if frame.returned.is_some() {
            frame.plan.instructions.len()
        } else {
            frame.next_pc(self.executor)?
        };
        if let Some(boundary) = &mut frame.safe_boundary {
            boundary.step_id = Some(step_id);
            boundary.variables = frame.variables.clone();
            self.emit_variables(frame.context, boundary, DebugSnapshotReason::StepBoundary);
        }
        frame.pc = next_pc;
        Ok(())
    }
}

pub(super) fn execute(
    executor: &Executor,
    plan: &MkExecutionPlan,
    options: ExecutionOptions,
    observe: &dyn Fn(ExecutionEvent),
) -> ExecResult {
    execute_root(
        executor,
        plan,
        None,
        &MkInvocationValues::new(),
        options,
        &|_, event| {
            // Preserve the standalone observer contract at its one adapter.
            if !matches!(
                event,
                ExecutionEvent::FrameEntered(_) | ExecutionEvent::FrameExited { .. }
            ) {
                observe(event);
            }
        },
    )
}

pub(super) fn execute_program(
    executor: &Executor,
    program: &MkCompiledProgram,
    arguments: &MkInvocationValues,
    options: ExecutionOptions,
    observe: &dyn Fn(ExecutionFrameContext, ExecutionEvent),
) -> ExecResult {
    let plan = program.plan(program.root_macro_id).ok_or_else(|| {
        ExecutionDiagnostic::new(DiagnosticKind::TargetNotFound, "Program root is missing")
    })?;
    execute_root(executor, plan, Some(program), arguments, options, observe)
}

fn execute_root(
    executor: &Executor,
    plan: &MkExecutionPlan,
    program: Option<&MkCompiledProgram>,
    arguments: &MkInvocationValues,
    options: ExecutionOptions,
    observe: &dyn Fn(ExecutionFrameContext, ExecutionEvent),
) -> ExecResult {
    let mut session = RootSession::new(executor, options, observe);
    if !plan.enabled {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            "Macro is disabled",
        ));
    }
    let variables = invocation::prepare_parameters(
        plan.signature.parameters(),
        plan.signature.outputs(),
        arguments,
    )?
    .into_variables(plan.signature.parameters())?;
    let frame = ExecutionFrame::with_variables(
        plan,
        options.mode,
        variables,
        ExecutionFrameContext {
            frame_id: 1,
            macro_id: plan.macro_id,
            caller_step_id: None,
            depth: 1,
        },
    );
    (session.observe)(
        frame.context,
        ExecutionEvent::FrameEntered(frame.snapshot()),
    );
    if let Some(boundary) = &frame.safe_boundary {
        session.emit_variables(frame.context, boundary, DebugSnapshotReason::RunStarted);
    }
    session.run_stack(frame, program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        MkErrorPolicy, MkMacro, MkRetry, MkStep, MkTextMode, compile, executor::fake::FakeBackend,
    };

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            action,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            metadata: Default::default(),
        }
    }
    fn plan(steps: Vec<MkStep>) -> MkExecutionPlan {
        compile(&MkMacro {
            id: 7,
            name: "frame".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            signature: Default::default(),
            steps,
        })
        .unwrap()
    }
    fn text(value: &str) -> MkAction {
        MkAction::Text(MkTextPayload {
            text: value.into(),
            mode: MkTextMode::Type,
        })
    }

    #[test]
    fn suspended_retry_resumes_same_repetition_without_reentering_breakpoint() {
        let mut repeated = step(1, text("${iteration}"));
        repeated.breakpoint = true;
        repeated.repeat = 2;
        repeated.delay_after_ms = 12;
        repeated.on_error = MkErrorPolicy::Retry(MkRetry {
            attempts: 2,
            delay_ms: 7,
        });
        let mut plan = plan(vec![repeated]);
        plan.playback.speed_percent = 200;
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "text:0",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "retry once"),
        );
        let control = Arc::new(RunControl::default());
        control.reset();
        let waiter = Arc::new(RecordingWaiter::default());
        let executor =
            Executor::with_waiter(fake.clone().backends(), control.clone(), waiter.clone());
        let events = Mutex::new(Vec::new());
        let observe = |_, event| {
            if matches!(event, ExecutionEvent::BreakpointHit { .. }) {
                control.resume();
            }
            events.lock().unwrap().push(event);
        };
        let mut session = RootSession::new(&executor, ExecutionOptions::debug(), &observe);
        let mut frame = ExecutionFrame::new(&plan, ExecutionMode::Debug);
        session.advance(&mut frame, None).unwrap();
        session.advance(&mut frame, None).unwrap();
        assert!(matches!(
            frame.phase,
            InstructionPhase::Attempt(AttemptState {
                repetition: 0,
                attempt: 2,
                attempts: 2
            })
        ));
        assert_eq!(frame.safe_boundary.as_ref().unwrap().step_id, None);
        assert_eq!(fake.events(), ["text:0"]);
        fake.failures.lock().unwrap().clear();
        session.run_frame(&mut frame).unwrap();
        assert_eq!(session.transitions, 1);
        assert_eq!(fake.events(), ["text:0", "text:0", "text:1"]);
        assert_eq!(
            waiter.sleeps(),
            [
                Duration::from_millis(7),
                Duration::from_millis(6),
                Duration::from_millis(6)
            ]
        );
        assert_eq!(
            frame.variables.get("iteration"),
            Some(&MkValue::Number(1.0))
        );
        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, ExecutionEvent::BreakpointHit { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, ExecutionEvent::StepStarted(1)))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, ExecutionEvent::StepFinished(1)))
                .count(),
            1
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ExecutionEvent::StepFailed(..)))
        );
    }

    #[test]
    fn pacing_cancellation_keeps_prior_safe_boundary_after_partial_effects() {
        let first = step(
            1,
            MkAction::SetVariable {
                name: "safe".into(),
                value: MkValue::Boolean(true),
            },
        );
        let mut second = step(
            2,
            MkAction::SetVariable {
                name: "partial".into(),
                value: MkValue::Boolean(true),
            },
        );
        second.delay_after_ms = 5;
        let plan = plan(vec![first, second]);
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::with_waiter(
            Arc::new(FakeBackend::default()).backends(),
            control.clone(),
            Arc::new(RecordingWaiter::stop_after(1)),
        );
        let events = Mutex::new(Vec::new());
        let error = executor
            .execute(&plan, ExecutionOptions::debug(), &|event| {
                events.lock().unwrap().push(event)
            })
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert!(!control.is_active());
        let events = events.lock().unwrap();
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, ExecutionEvent::StepFinished(2)))
        );
        let ExecutionEvent::DebugVariables {
            step_id,
            variables,
            reason,
        } = events.last().unwrap()
        else {
            panic!("expected terminal safe snapshot")
        };
        assert_eq!(
            (*step_id, *reason),
            (Some(1), DebugSnapshotReason::RunCancelled)
        );
        assert_eq!(variables.get("safe"), Some(&MkValue::Boolean(true)));
        assert!(!variables.contains_key("partial"));
    }

    #[test]
    fn root_budget_and_owned_input_survive_frame_completion() {
        let first_plan = plan(vec![step(1, MkAction::KeyDown(MkKey::Control))]);
        let second_plan = plan(vec![step(1, text("must not run"))]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control.clone());
        let mut session = RootSession::new(&executor, ExecutionOptions::normal(), &|_, _| {});
        session.transitions = Executor::MAX_CONTROL_TRANSITIONS - 1;
        let mut first = ExecutionFrame::new(&first_plan, ExecutionMode::Normal);
        session.run_frame(&mut first).unwrap();
        assert!(first.safe_boundary.is_none());
        drop(first);
        assert!(control.is_active());
        assert_eq!(fake.events(), ["key_down:Control"]);
        let mut second = ExecutionFrame::new(&second_plan, ExecutionMode::Normal);
        let error = session.run_frame(&mut second).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::IterationLimit);
        assert_eq!(session.transitions, Executor::MAX_CONTROL_TRANSITIONS + 1);
        assert_eq!(fake.events(), ["key_down:Control"]);
        drop(session);
        assert_eq!(fake.events(), ["key_down:Control", "key_up:Control"]);
        assert!(!control.is_active());
    }

    fn macro_with(id: u64, steps: Vec<MkStep>) -> MkMacro {
        MkMacro {
            id,
            name: format!("macro {id}"),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps,
        }
    }
    fn program(macros: Vec<MkMacro>) -> MkCompiledProgram {
        crate::mkmacro::compile_program(
            &crate::mkmacro::MkMacroDocument {
                macros,
                ..Default::default()
            },
            1,
        )
        .unwrap()
    }
    fn call(id: u64) -> MkAction {
        MkAction::CallMacro(crate::mkmacro::MkCallMacroPayload {
            macro_id: id,
            ..Default::default()
        })
    }
    fn set(name: &str, value: &str) -> MkAction {
        MkAction::SetVariable {
            name: name.into(),
            value: MkValue::String(value.into()),
        }
    }

    #[test]
    fn nested_calls_keep_identity_and_locals_isolated_with_one_root_input_owner() {
        let program = program(vec![
            macro_with(
                1,
                vec![
                    step(1, set("x", "parent")),
                    step(2, MkAction::KeyDown(MkKey::Control)),
                    step(3, text("A-before")),
                    step(4, call(2)),
                    step(5, text("A-after:${x}:${macro.id}:${macro.name}")),
                ],
            ),
            macro_with(
                2,
                vec![
                    step(1, set("x", "child")),
                    step(2, text("B:${x}:${macro.id}:${macro.name}")),
                    step(3, call(3)),
                    step(4, text("B-after:${x}")),
                ],
            ),
            macro_with(3, vec![step(1, text("C:${macro.id}:${macro.name}"))]),
        ]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let contexts = Mutex::new(Vec::new());
        Executor::new(fake.clone().backends(), control.clone())
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::normal(),
                &|context, event| {
                    assert!(!matches!(
                        event,
                        ExecutionEvent::DebugVariables { .. }
                            | ExecutionEvent::BreakpointHit { .. }
                    ));
                    contexts.lock().unwrap().push(context);
                },
            )
            .unwrap();
        assert_eq!(
            fake.events(),
            [
                "key_down:Control",
                "text:A-before",
                "text:B:child:2:macro 2",
                "text:C:3:macro 3",
                "text:B-after:child",
                "text:A-after:parent:1:macro 1",
                "key_up:Control"
            ]
        );
        assert!(
            contexts
                .lock()
                .unwrap()
                .iter()
                .any(|context| context.macro_id == 3
                    && context.depth == 3
                    && context.caller_step_id == Some(3))
        );
        assert!(!control.is_active());
    }

    #[test]
    fn call_arguments_keep_ab_order_defaults_builtins_and_same_name_locals_isolated() {
        use crate::mkmacro::{
            MkCallArgumentBinding, MkCallOutputBinding, MkMacroOutput, MkMacroParameter,
            MkReturnPayload, MkReturnValueBinding, MkSignatureId, MkValueSource, MkValueType,
        };
        let parameter = |id: u64, name: &str, default_value: Option<MkValue>| MkMacroParameter {
            id: MkSignatureId(id),
            name: name.into(),
            value_type: MkValueType::String,
            description: String::new(),
            default_value,
        };
        let mut call_b = crate::mkmacro::MkCallMacroPayload {
            macro_id: 2,
            ..Default::default()
        };
        call_b.arguments = vec![
            MkCallArgumentBinding {
                parameter_id: MkSignatureId(1),
                source: MkValueSource::Literal(MkValue::String("literal:${same}".into())),
            },
            MkCallArgumentBinding {
                parameter_id: MkSignatureId(2),
                source: MkValueSource::Variable {
                    name: "same".into(),
                },
            },
            MkCallArgumentBinding {
                parameter_id: MkSignatureId(4),
                source: MkValueSource::Variable {
                    name: "macro.name".into(),
                },
            },
        ];
        call_b.outputs.push(MkCallOutputBinding {
            output_id: MkSignatureId(5),
            caller_variable: "mapped".into(),
        });
        let root = macro_with(
            1,
            vec![
                step(1, set("same", "A-local")),
                step(
                    2,
                    MkAction::Text(MkTextPayload {
                        text: "A-before".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
                step(3, MkAction::CallMacro(call_b)),
                step(4, text("A-after:${same}:${mapped}")),
            ],
        );
        let mut child = macro_with(
            2,
            vec![
                step(1, text("B:${literal}:${same}:${defaulted}:${source_macro}")),
                step(2, set("same", "B-mutated")),
                step(3, text("B-after:${same}")),
                step(
                    4,
                    MkAction::Return(MkReturnPayload {
                        outputs: vec![
                            MkReturnValueBinding {
                                output_id: MkSignatureId(5),
                                source: MkValueSource::Variable {
                                    name: "same".into(),
                                },
                            },
                            MkReturnValueBinding {
                                output_id: MkSignatureId(6),
                                source: MkValueSource::Literal(MkValue::String("discarded".into())),
                            },
                        ],
                    }),
                ),
            ],
        );
        child.signature.parameters = vec![
            parameter(1, "literal", None),
            parameter(2, "same", None),
            parameter(
                3,
                "defaulted",
                Some(MkValue::String("default:${same}".into())),
            ),
            parameter(4, "source_macro", None),
        ];
        child.signature.outputs = vec![
            MkMacroOutput {
                id: MkSignatureId(5),
                name: "mapped".into(),
                value_type: MkValueType::String,
                description: String::new(),
            },
            MkMacroOutput {
                id: MkSignatureId(6),
                name: "discarded".into(),
                value_type: MkValueType::String,
                description: String::new(),
            },
        ];
        let program = program(vec![root, child]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();

        Executor::new(fake.clone().backends(), control)
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::normal(),
                &|_, _| {},
            )
            .unwrap();

        assert_eq!(
            fake.events(),
            [
                "text:A-before",
                "text:B:literal:A-local:A-local:default:${same}:macro 1",
                "text:B-after:B-mutated",
                "text:A-after:A-local:B-mutated",
            ]
        );
    }

    #[test]
    fn early_return_and_procedure_fallthrough_resume_the_exact_caller_position() {
        let root = macro_with(
            1,
            vec![
                step(1, text("A-before")),
                step(2, call(2)),
                step(3, text("A-between")),
                step(4, call(3)),
                step(5, text("A-after")),
                step(6, MkAction::Return(Default::default())),
                step(7, text("root-unreachable")),
            ],
        );
        let child_return = macro_with(
            2,
            vec![
                step(1, text("B-before")),
                step(2, MkAction::Return(Default::default())),
                step(3, text("B-unreachable")),
            ],
        );
        let child_fallthrough = macro_with(3, vec![step(1, text("C-natural-end"))]);
        let program = program(vec![root, child_return, child_fallthrough]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();

        Executor::new(fake.clone().backends(), control)
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::normal(),
                &|_, _| {},
            )
            .unwrap();

        assert_eq!(
            fake.events(),
            [
                "text:A-before",
                "text:B-before",
                "text:A-between",
                "text:C-natural-end",
                "text:A-after",
            ]
        );
    }

    #[test]
    fn continued_child_failure_preserves_caller_locals_outputs_and_root_owned_input() {
        use crate::mkmacro::{
            MkCallOutputBinding, MkMacroOutput, MkReturnPayload, MkReturnValueBinding,
            MkSignatureId, MkValueSource, MkValueType,
        };
        let mut call_step = step(3, call(2));
        call_step.on_error = MkErrorPolicy::Continue;
        let MkAction::CallMacro(call) = &mut call_step.action else {
            unreachable!()
        };
        call.outputs.push(MkCallOutputBinding {
            output_id: MkSignatureId(1),
            caller_variable: "result".into(),
        });
        let root = macro_with(
            1,
            vec![
                step(1, MkAction::KeyDown(MkKey::Control)),
                step(2, set("result", "caller")),
                call_step,
                step(4, text("A-after:${result}:${last_action_success}")),
            ],
        );
        let mut child = macro_with(
            2,
            vec![
                step(1, text("B-fails")),
                step(
                    2,
                    MkAction::Return(MkReturnPayload {
                        outputs: vec![MkReturnValueBinding {
                            output_id: MkSignatureId(1),
                            source: MkValueSource::Literal(MkValue::String("child".into())),
                        }],
                    }),
                ),
            ],
        );
        child.signature.outputs.push(MkMacroOutput {
            id: MkSignatureId(1),
            name: "result".into(),
            value_type: MkValueType::String,
            description: String::new(),
        });
        let program = program(vec![root, child]);
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "text:B-fails",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected child failure"),
        );
        let control = Arc::new(RunControl::default());
        control.reset();

        Executor::new(fake.clone().backends(), control.clone())
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::normal(),
                &|_, _| {},
            )
            .unwrap();

        assert_eq!(
            fake.events(),
            [
                "key_down:Control",
                "text:B-fails",
                "text:A-after:caller:false",
                "key_up:Control",
            ]
        );
        assert!(!control.is_active());
    }

    #[test]
    fn call_retry_and_repetition_use_fresh_parameters_and_callee_playback() {
        use crate::mkmacro::{
            MkCallOutputBinding, MkMacroOutput, MkMacroParameter, MkReturnPayload,
            MkReturnValueBinding, MkSignatureId, MkValueSource, MkValueType,
        };
        let mut repeated = step(2, call(2));
        repeated.repeat = 2;
        repeated.delay_after_ms = 6;
        repeated.on_error = MkErrorPolicy::Retry(MkRetry {
            attempts: 2,
            delay_ms: 7,
        });
        let MkAction::CallMacro(payload) = &mut repeated.action else {
            unreachable!()
        };
        payload.outputs.push(MkCallOutputBinding {
            output_id: MkSignatureId(2),
            caller_variable: "x".into(),
        });
        let mut root = macro_with(
            1,
            vec![
                step(1, set("x", "parent")),
                repeated,
                step(3, text("result:${x}")),
            ],
        );
        root.playback.speed_percent = 50;
        let mut first = step(1, text("${x}"));
        first.delay_after_ms = 8;
        let mut child = macro_with(
            2,
            vec![
                first,
                step(2, set("x", "changed")),
                step(3, text("fail once")),
                step(
                    4,
                    MkAction::Return(MkReturnPayload {
                        outputs: vec![MkReturnValueBinding {
                            output_id: MkSignatureId(2),
                            source: MkValueSource::Variable { name: "x".into() },
                        }],
                    }),
                ),
                step(5, text("unreachable")),
            ],
        );
        child.playback.speed_percent = 200;
        child.signature.parameters.push(MkMacroParameter {
            id: MkSignatureId(1),
            name: "x".into(),
            value_type: MkValueType::String,
            description: String::new(),
            default_value: Some(MkValue::String("fresh".into())),
        });
        child.signature.outputs.push(MkMacroOutput {
            id: MkSignatureId(2),
            name: "result".into(),
            value_type: MkValueType::String,
            description: String::new(),
        });
        let program = program(vec![root, child]);
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "text:fail once",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "fail first invocation"),
        );
        let control = Arc::new(RunControl::default());
        control.reset();
        let waiter = Arc::new(RecordingWaiter::default());
        let starts = Mutex::new(Vec::new());
        Executor::with_waiter(fake.clone().backends(), control, waiter.clone())
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::debug(),
                &|context, event| {
                    if context.macro_id == 2 {
                        if let ExecutionEvent::DebugVariables {
                            reason: DebugSnapshotReason::RunStarted,
                            variables,
                            ..
                        } = &event
                        {
                            starts
                                .lock()
                                .unwrap()
                                .push((context.frame_id, variables["x"].clone()));
                        }
                        if matches!(event, ExecutionEvent::StepFailed(..)) {
                            fake.failures.lock().unwrap().clear();
                        }
                    }
                },
            )
            .unwrap();
        assert_eq!(
            fake.events(),
            [
                "text:fresh",
                "text:fail once",
                "text:fresh",
                "text:fail once",
                "text:fresh",
                "text:fail once",
                "text:result:changed"
            ]
        );
        assert_eq!(
            *starts.lock().unwrap(),
            [
                (2, MkValue::String("fresh".into())),
                (3, MkValue::String("fresh".into())),
                (4, MkValue::String("fresh".into()))
            ]
        );
        assert_eq!(
            waiter.sleeps(),
            [4, 7, 4, 12, 4, 12].map(Duration::from_millis)
        );
    }

    #[test]
    fn stopping_call_or_return_after_step_started_emits_failure_without_effects() {
        for root_action in [call(2), MkAction::Return(Default::default())] {
            let mut root_step = step(2, root_action);
            root_step.on_error = MkErrorPolicy::Retry(MkRetry {
                attempts: 3,
                delay_ms: 7,
            });
            let program = program(vec![
                macro_with(1, vec![step(1, set("safe", "settled")), root_step]),
                macro_with(2, vec![]),
            ]);
            let fake = Arc::new(FakeBackend::default());
            let control = Arc::new(RunControl::default());
            control.reset();
            let waiter = Arc::new(RecordingWaiter::default());
            let events = Mutex::new(Vec::new());
            let error =
                Executor::with_waiter(fake.clone().backends(), control.clone(), waiter.clone())
                    .execute_program(
                        &program,
                        &MkInvocationValues::new(),
                        ExecutionOptions::debug(),
                        &|context, event| {
                            assert_eq!(context.macro_id, 1, "cancelled Call must not push a child");
                            if matches!(event, ExecutionEvent::StepStarted(2)) {
                                control.stop();
                            }
                            events.lock().unwrap().push(event);
                        },
                    )
                    .unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::Cancelled);
            assert!(fake.events().is_empty());
            assert!(waiter.sleeps().is_empty());
            let events = events.lock().unwrap();
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, ExecutionEvent::StepFailed(2, _)))
                    .count(),
                1
            );
            assert!(matches!(
                events
                    .iter()
                    .rev()
                    .find(|event| matches!(event, ExecutionEvent::DebugVariables { .. }))
                    .unwrap(),
                ExecutionEvent::DebugVariables {
                    step_id: Some(1),
                    reason: DebugSnapshotReason::RunCancelled,
                    ..
                }
            ));
        }
    }

    #[test]
    fn stop_after_child_return_discards_outputs_and_ignores_call_continue_policy() {
        use crate::mkmacro::{
            MkCallOutputBinding, MkMacroOutput, MkReturnPayload, MkReturnValueBinding,
            MkSignatureId, MkValueSource, MkValueType,
        };
        let mut root_call = step(2, call(2));
        root_call.on_error = MkErrorPolicy::Continue;
        let MkAction::CallMacro(payload) = &mut root_call.action else {
            unreachable!()
        };
        payload.outputs.push(MkCallOutputBinding {
            output_id: MkSignatureId(1),
            caller_variable: "x".into(),
        });
        let mut child = macro_with(
            2,
            vec![step(
                1,
                MkAction::Return(MkReturnPayload {
                    outputs: vec![MkReturnValueBinding {
                        output_id: MkSignatureId(1),
                        source: MkValueSource::Literal(MkValue::String("child".into())),
                    }],
                }),
            )],
        );
        child.signature.outputs.push(MkMacroOutput {
            id: MkSignatureId(1),
            name: "result".into(),
            value_type: MkValueType::String,
            description: String::new(),
        });
        let program = program(vec![
            macro_with(
                1,
                vec![
                    step(1, set("x", "parent")),
                    root_call,
                    step(3, text("must not execute")),
                ],
            ),
            child,
        ]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let terminal = Mutex::new(None);
        let error = Executor::new(fake.clone().backends(), control.clone())
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::debug(),
                &|context, event| {
                    if context.macro_id == 2
                        && matches!(
                            event,
                            ExecutionEvent::DebugVariables {
                                reason: DebugSnapshotReason::RunFinished,
                                ..
                            }
                        )
                    {
                        control.stop();
                    }
                    if context.macro_id == 1
                        && let ExecutionEvent::DebugVariables {
                            reason: DebugSnapshotReason::RunCancelled,
                            variables,
                            ..
                        } = event
                    {
                        *terminal.lock().unwrap() = Some(variables);
                    }
                },
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert!(fake.events().is_empty());
        assert_eq!(
            terminal.lock().unwrap().as_ref().unwrap()["x"],
            MkValue::String("parent".into())
        );
    }

    #[test]
    fn runtime_return_contract_and_mapping_failure_never_commit_partial_outputs() {
        use crate::mkmacro::{
            MkCallOutputBinding, MkInvocationSubset, MkMacroOutput, MkReturnPayload,
            MkReturnValueBinding, MkSignatureId, MkValueSource, MkValueType, apply_root_subset,
        };
        let mut child = macro_with(
            2,
            vec![step(
                1,
                MkAction::Return(MkReturnPayload {
                    outputs: vec![MkReturnValueBinding {
                        output_id: MkSignatureId(1),
                        source: MkValueSource::Literal(MkValue::String("child".into())),
                    }],
                }),
            )],
        );
        child.signature.outputs.push(MkMacroOutput {
            id: MkSignatureId(1),
            name: "result".into(),
            value_type: MkValueType::String,
            description: String::new(),
        });
        let mut root_call = step(2, call(2));
        root_call.on_error = MkErrorPolicy::Continue;
        let MkAction::CallMacro(payload) = &mut root_call.action else {
            unreachable!()
        };
        payload.outputs.push(MkCallOutputBinding {
            output_id: MkSignatureId(1),
            caller_variable: "x".into(),
        });
        let mut program = program(vec![
            macro_with(
                1,
                vec![
                    step(1, set("x", "parent")),
                    root_call,
                    step(3, text("${x}")),
                ],
            ),
            child,
        ]);
        // Emulate a malformed caller bypassing document admission; mapping IDs
        // must be checked before executing or committing any child output.
        let root = program.root_plan_mut().unwrap();
        let instruction = &mut Arc::make_mut(&mut root.instructions)[1];
        let MkAction::CallMacro(payload) = &mut Arc::make_mut(&mut instruction.step).action else {
            unreachable!()
        };
        payload.outputs.push(MkCallOutputBinding {
            output_id: MkSignatureId(999),
            caller_variable: "other".into(),
        });
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        Executor::new(fake.clone().backends(), control)
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::normal(),
                &|_, _| {},
            )
            .unwrap();
        assert_eq!(fake.events(), ["text:parent"]);
        assert_eq!(program.plan(2).unwrap().instructions.len(), 1);
        apply_root_subset(&mut program, &MkInvocationSubset::Selected(vec![3])).unwrap();
        assert_eq!(program.plan(1).unwrap().instructions.len(), 1);
        assert_eq!(program.plan(2).unwrap().instructions.len(), 1);
        let mut output_root = program.plan(2).unwrap().as_ref().clone();
        output_root.instructions = Arc::from([]);
        output_root.step_to_instruction.clear();
        let control = Arc::new(RunControl::default());
        control.reset();
        assert_eq!(
            Executor::new(fake.backends(), control)
                .execute(&output_root, ExecutionOptions::normal(), &|_| {})
                .unwrap_err()
                .kind,
            DiagnosticKind::InvalidPlan
        );
    }

    #[test]
    fn malformed_recursive_program_is_depth_bounded_and_shares_transition_budget() {
        let mut program = program(vec![macro_with(1, vec![step(1, text("placeholder"))])]);
        let root = program.root_plan_mut().unwrap();
        Arc::make_mut(&mut Arc::make_mut(&mut root.instructions)[0].step).action = call(1);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control.clone());
        let maximum_depth = Mutex::new(0);
        let error = executor
            .execute_program(
                &program,
                &MkInvocationValues::new(),
                ExecutionOptions::normal(),
                &|context, _| {
                    let mut depth = maximum_depth.lock().unwrap();
                    *depth = (*depth).max(context.depth);
                },
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::IterationLimit);
        assert_eq!(error.context["limit"], "64");
        assert_eq!(*maximum_depth.lock().unwrap(), 64);
        control.reset();
        let mut session = RootSession::new(&executor, ExecutionOptions::normal(), &|_, _| {});
        session.transitions = Executor::MAX_CONTROL_TRANSITIONS - 1;
        let root = ExecutionFrame::new(program.plan(1).unwrap(), ExecutionMode::Normal);
        let error = session.run_stack(root, Some(&program)).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::IterationLimit);
        assert_eq!(error.context["limit"], "100000");
        assert_eq!(session.transitions, Executor::MAX_CONTROL_TRANSITIONS + 1);
        assert!(fake.events().is_empty());
    }

    struct ReleaseObserver {
        fake: Arc<FakeBackend>,
        control: Arc<RunControl>,
        active_on_release: Mutex<Vec<bool>>,
    }
    impl InputBackend for ReleaseObserver {
        fn key_down(&self, key: &MkKey) -> ExecResult {
            self.fake.key_down(key)
        }
        fn key_up(&self, key: &MkKey) -> ExecResult {
            self.active_on_release
                .lock()
                .unwrap()
                .push(self.control.is_active());
            self.fake.key_up(key)
        }
        fn button_down(&self, button: MkMouseButton) -> ExecResult {
            self.fake.button_down(button)
        }
        fn button_up(&self, button: MkMouseButton) -> ExecResult {
            self.active_on_release
                .lock()
                .unwrap()
                .push(self.control.is_active());
            self.fake.button_up(button)
        }
        fn move_mouse(&self, point: MkPoint) -> ExecResult {
            self.fake.move_mouse(point)
        }
        fn cursor_position(&self) -> ExecResult<MkPoint> {
            self.fake.cursor_position()
        }
        fn scroll(&self, axis: MkMouseScrollAxis, delta: i32) -> ExecResult {
            self.fake.scroll(axis, delta)
        }
        fn text(&self, payload: &MkTextPayload) -> ExecResult {
            self.fake.text(payload)
        }
    }

    #[test]
    fn root_cleanup_releases_input_before_activity_on_success_and_panic() {
        for panic in [false, true] {
            let fake = Arc::new(FakeBackend::default());
            let control = Arc::new(RunControl::default());
            control.reset();
            let input = Arc::new(ReleaseObserver {
                fake: fake.clone(),
                control: control.clone(),
                active_on_release: Mutex::new(Vec::new()),
            });
            let mut backends = fake.clone().backends();
            backends.input = input.clone();
            let executor = Executor::new(backends, control.clone());
            let plan = plan(vec![
                step(1, MkAction::KeyDown(MkKey::Control)),
                step(2, MkAction::MouseDown(MkMouseButton::Left)),
                step(3, text("end")),
            ]);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                executor.execute(&plan, ExecutionOptions::normal(), &|event| {
                    if panic && matches!(event, ExecutionEvent::StepStarted(3)) {
                        panic!("injected observer panic");
                    }
                })
            }));
            assert_eq!(result.is_err(), panic);
            if let Ok(result) = result {
                result.unwrap();
            }
            assert_eq!(*input.active_on_release.lock().unwrap(), [true, true]);
            assert!(
                fake.events()
                    .ends_with(&["button_up:Left".into(), "key_up:Control".into()])
            );
            assert!(!control.is_active());
        }
    }
}
