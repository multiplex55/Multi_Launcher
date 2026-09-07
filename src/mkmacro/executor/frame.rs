//! Per-invocation continuation and root-owned execution resources. Effects keep
//! using Executor's existing backend boundary; a frame never owns input cleanup
//! or a separate RunControl.
use super::*;

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
    Pace { repetition: u32 },
    Finish { error: Option<ExecutionDiagnostic> },
}

struct ExecutionFrame<'plan> {
    plan: &'plan MkExecutionPlan,
    pc: usize,
    variables: RuntimeVariables,
    loops: HashMap<usize, u32>,
    phase: InstructionPhase,
    // Partial effects and failed condition evaluation must never overwrite the
    // last settled debugger boundary. Normal execution needs no snapshot clone.
    safe_boundary: Option<SafeBoundary>,
}

impl<'plan> ExecutionFrame<'plan> {
    fn new(plan: &'plan MkExecutionPlan, mode: ExecutionMode) -> Self {
        let variables: RuntimeVariables = [
            ("macro.id".into(), MkValue::Number(plan.macro_id as f64)),
            ("last_action_success".into(), MkValue::Boolean(true)),
        ]
        .into_iter()
        .collect();
        let safe_boundary = (mode == ExecutionMode::Debug).then(|| SafeBoundary {
            step_id: None,
            variables: variables.clone(),
        });
        Self {
            plan,
            pc: 0,
            variables,
            loops: HashMap::new(),
            phase: InstructionPhase::Enter,
            safe_boundary,
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
    observe: &'observer dyn Fn(ExecutionEvent),
    transitions: u64,
}

impl<'executor, 'observer> RootSession<'executor, 'observer> {
    fn new(
        executor: &'executor Executor,
        options: ExecutionOptions,
        observe: &'observer dyn Fn(ExecutionEvent),
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
        }
    }

    fn emit_variables(&self, boundary: &SafeBoundary, reason: DebugSnapshotReason) {
        (self.observe)(ExecutionEvent::DebugVariables {
            step_id: boundary.step_id,
            variables: boundary.variables.clone(),
            reason,
        });
    }

    fn run_frame(&mut self, frame: &mut ExecutionFrame<'_>) -> ExecResult {
        while frame.pc < frame.plan.instructions.len() {
            self.advance(frame)?;
        }
        Ok(())
    }

    fn advance(&mut self, frame: &mut ExecutionFrame<'_>) -> ExecResult {
        match std::mem::replace(&mut frame.phase, InstructionPhase::Enter) {
            InstructionPhase::Enter => self.enter_instruction(frame),
            InstructionPhase::Attempt(attempt) => {
                let step = &frame.plan.instructions[frame.pc].step;
                tracing::debug!(
                    macro_id = frame.plan.macro_id,
                    step_id = step.id,
                    attempt = attempt.attempt,
                    "executing macro step"
                );
                let result = self.executor.action(
                    frame.plan.macro_id,
                    &step.action,
                    &frame.plan.playback,
                    &mut frame.variables,
                    &mut self.input,
                );
                self.complete_attempt(frame, attempt, result)
            }
            InstructionPhase::Pace { repetition } => self.pace_repetition(frame, repetition),
            InstructionPhase::Finish { error } => self.finish_instruction(frame, error),
        }
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
            (self.observe)(ExecutionEvent::StepSkipped(step.id));
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
            (self.observe)(ExecutionEvent::BreakpointHit {
                step_id: step.id,
                variables: frame.variables.clone(),
            });
            self.executor.control.checkpoint()?;
        }
        (self.observe)(ExecutionEvent::StepStarted(step.id));
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
                if attempt.attempt < attempt.attempts
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
            (self.observe)(ExecutionEvent::StepFailed(step_id, error.clone()));
            if error.kind == DiagnosticKind::Cancelled
                || !matches!(step.on_error, super::super::MkErrorPolicy::Continue)
            {
                return Err(error);
            }
        } else {
            let outcome = StepOutcome::for_action(&step.action, &frame.variables);
            if outcome.last_image_found.is_some() {
                (self.observe)(ExecutionEvent::StepOutcome(step_id, outcome));
            }
            (self.observe)(ExecutionEvent::StepFinished(step_id));
        }
        // StepFinished historically precedes If/While evaluation, but a safe
        // snapshot is published only after that evaluation and jump succeed.
        let next_pc = frame.next_pc(self.executor)?;
        if let Some(boundary) = &mut frame.safe_boundary {
            boundary.step_id = Some(step_id);
            boundary.variables = frame.variables.clone();
            self.emit_variables(boundary, DebugSnapshotReason::StepBoundary);
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
    let mut session = RootSession::new(executor, options, observe);
    let mut frame = ExecutionFrame::new(plan, options.mode);
    if let Some(boundary) = &frame.safe_boundary {
        session.emit_variables(boundary, DebugSnapshotReason::RunStarted);
    }
    let result = session.run_frame(&mut frame);
    let reason = match &result {
        Ok(()) => DebugSnapshotReason::RunFinished,
        Err(error) if error.kind == DiagnosticKind::Cancelled => DebugSnapshotReason::RunCancelled,
        Err(_) => DebugSnapshotReason::RunFailed,
    };
    if let Some(boundary) = &frame.safe_boundary {
        session.emit_variables(boundary, reason);
    }
    result
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
        let observe = |event| {
            if matches!(event, ExecutionEvent::BreakpointHit { .. }) {
                control.resume();
            }
            events.lock().unwrap().push(event);
        };
        let mut session = RootSession::new(&executor, ExecutionOptions::debug(), &observe);
        let mut frame = ExecutionFrame::new(&plan, ExecutionMode::Debug);
        session.advance(&mut frame).unwrap();
        session.advance(&mut frame).unwrap();
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
        let mut session = RootSession::new(&executor, ExecutionOptions::normal(), &|_| {});
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
