//! Nonblocking pre-run input. The slot remains occupied while the GUI owns a
//! request; neither queued input nor a visible prompt reserves playback.
use super::executor::ExecResult;
use super::{
    DiagnosticKind, ExecutionDiagnostic, MkInvocation, MkInvocationValues, MkMacroParameter,
    runtime::MacroRuntime,
};
use std::sync::{Arc, Mutex, OnceLock, Weak};

type Repaint = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone)]
pub struct InvocationPromptRequest {
    pub id: u64,
    pub runtime_generation: u64,
    pub invocation: MkInvocation,
    pub macro_name: String,
    pub macro_description: String,
    pub parameters: Vec<MkMacroParameter>,
    pub prepared_values: MkInvocationValues,
}

struct QueuedInvocation {
    request: InvocationPromptRequest,
    runtime: Weak<MacroRuntime>,
}
struct Occupied {
    token: u64,
    queued: Option<QueuedInvocation>,
    submitting: bool,
}
#[derive(Default)]
struct State {
    next_id: u64,
    registration: Option<(u64, Repaint)>,
    occupied: Option<Occupied>,
    notice: Option<String>,
}
#[derive(Default)]
pub struct InvocationPromptBroker {
    state: Mutex<State>,
}

/// Availability belongs to a GUI lifetime. Replacing a registration invalidates
/// its requests; dropping an old guard cannot unregister its replacement.
pub struct InvocationGuiRegistration {
    broker: Arc<InvocationPromptBroker>,
    id: u64,
}
impl InvocationGuiRegistration {
    /// Explicitly ends GUI availability and cancels the request owned by this
    /// registration. The token check makes repeated shutdown and stale guards
    /// harmless.
    pub fn unregister(&self) {
        let removed = {
            let mut state = self.broker.state.lock().unwrap();
            if state
                .registration
                .as_ref()
                .is_some_and(|(id, _)| *id == self.id)
            {
                state.occupied = None;
                state.notice = None;
                state.registration.take()
            } else {
                None
            }
        };
        drop(removed);
    }

    pub fn take_pending(&self) -> Option<PendingInvocation> {
        let mut state = self.broker.state.lock().unwrap();
        if !state
            .registration
            .as_ref()
            .is_some_and(|(id, _)| *id == self.id)
        {
            return None;
        }
        let queued = state.occupied.as_mut()?.queued.take()?;
        Some(PendingInvocation {
            request: queued.request,
            runtime: queued.runtime,
            broker: self.broker.clone(),
        })
    }
    pub fn take_notice(&self) -> Option<String> {
        let mut state = self.broker.state.lock().unwrap();
        if state
            .registration
            .as_ref()
            .is_some_and(|(id, _)| *id == self.id)
        {
            state.notice.take()
        } else {
            None
        }
    }
}
impl Drop for InvocationGuiRegistration {
    fn drop(&mut self) {
        self.unregister();
    }
}

impl InvocationPromptBroker {
    pub fn register_gui(self: &Arc<Self>, repaint: Repaint) -> InvocationGuiRegistration {
        let (id, previous) = {
            let mut state = self.state.lock().unwrap();
            state.next_id += 1;
            let id = state.next_id;
            state.occupied = None;
            state.notice = None;
            (id, state.registration.replace((id, repaint)))
        };
        drop(previous);
        InvocationGuiRegistration {
            broker: self.clone(),
            id,
        }
    }
    pub fn ensure_idle(&self) -> ExecResult {
        if self.state.lock().unwrap().occupied.is_some() {
            Err(prompt_error(
                "Another macro parameter request is already open",
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn enqueue(
        self: &Arc<Self>,
        runtime: &Arc<MacroRuntime>,
        mut request: InvocationPromptRequest,
    ) -> ExecResult<u64> {
        let (id, repaint) = {
            let mut state = self.state.lock().unwrap();
            if state.occupied.is_some() {
                return Err(prompt_error(
                    "Another macro parameter request is already open",
                ));
            }
            let repaint = state
                .registration
                .as_ref()
                .map(|(_, cb)| cb.clone())
                .ok_or_else(|| {
                    prompt_error("Macro parameters require an available launcher GUI")
                })?;
            state.next_id += 1;
            request.id = state.next_id;
            let id = request.id;
            state.occupied = Some(Occupied {
                token: id,
                queued: Some(QueuedInvocation {
                    request,
                    runtime: Arc::downgrade(runtime),
                }),
                submitting: false,
            });
            (id, repaint)
        };
        repaint();
        Ok(id)
    }
    fn cancel(&self, token: u64) {
        let mut state = self.state.lock().unwrap();
        if state
            .occupied
            .as_ref()
            .is_some_and(|slot| slot.token == token)
        {
            state.occupied = None;
        }
    }
    /// Retain one bounded visible notice, also logged by the hotkey caller.
    pub fn report_error(&self, message: String) {
        let repaint = {
            let mut state = self.state.lock().unwrap();
            let repaint = state.registration.as_ref().map(|(_, cb)| cb.clone());
            if repaint.is_some() {
                state.notice = Some(message);
            }
            repaint
        };
        if let Some(repaint) = repaint {
            repaint();
        }
    }
}

pub struct PendingInvocation {
    request: InvocationPromptRequest,
    runtime: Weak<MacroRuntime>,
    broker: Arc<InvocationPromptBroker>,
}
impl PendingInvocation {
    pub fn request(&self) -> &InvocationPromptRequest {
        &self.request
    }
    pub fn is_active(&self) -> bool {
        self.broker
            .state
            .lock()
            .unwrap()
            .occupied
            .as_ref()
            .is_some_and(|slot| slot.token == self.request.id)
    }
    pub fn submit(&mut self, values: MkInvocationValues) -> ExecResult {
        {
            let mut state = self.broker.state.lock().unwrap();
            let slot = state
                .occupied
                .as_mut()
                .filter(|slot| slot.token == self.request.id && !slot.submitting)
                .ok_or_else(|| prompt_error("This macro parameter request is no longer active"))?;
            // Confirmation wins cancellation at this token claim. Runtime work
            // stays outside the broker lock; concurrent submissions stay busy.
            slot.submitting = true;
        }
        let result = self
            .runtime
            .upgrade()
            .ok_or_else(|| prompt_error("The macro runtime was replaced or closed"))
            .and_then(|runtime| runtime.confirm_invocation(&self.request, values));
        if result.is_ok() {
            self.broker.cancel(self.request.id);
        } else {
            let mut state = self.broker.state.lock().unwrap();
            if let Some(slot) = state
                .occupied
                .as_mut()
                .filter(|s| s.token == self.request.id)
            {
                slot.submitting = false;
            }
        }
        result
    }
}
impl Drop for PendingInvocation {
    fn drop(&mut self) {
        self.broker.cancel(self.request.id);
    }
}
fn prompt_error(message: &str) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::RuntimeUnavailable, message)
}
pub fn production_invocation_prompt_broker() -> Arc<InvocationPromptBroker> {
    static BROKER: OnceLock<Arc<InvocationPromptBroker>> = OnceLock::new();
    BROKER
        .get_or_init(|| Arc::new(InvocationPromptBroker::default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        executor::fake::FakeBackend,
        runtime::{InvocationDisposition, RuntimeCommand, RuntimeState},
        *,
    };
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    fn fixture(
        default: Option<MkValue>,
    ) -> (
        tempfile::TempDir,
        Arc<MkMacroStore>,
        Arc<MacroRuntime>,
        Arc<FakeBackend>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let steps = [10, 20]
            .into_iter()
            .map(|id| MkStep {
                id,
                enabled: true,
                breakpoint: false,
                metadata: Default::default(),
                repeat: 1,
                on_error: MkErrorPolicy::Stop,
                delay_after_ms: 0,
                action: MkAction::Text(MkTextPayload {
                    text: "${input}".into(),
                    mode: MkTextMode::Type,
                }),
            })
            .collect();
        store
            .save(MkMacroDocument {
                macros: vec![MkMacro {
                    id: 1,
                    name: "Parameterized".into(),
                    description: "Prompt description".into(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: Default::default(),
                    steps,
                    signature: MkMacroSignature {
                        parameters: vec![MkMacroParameter {
                            id: MkSignatureId(7),
                            name: "input".into(),
                            description: "Input description".into(),
                            value_type: MkValueType::String,
                            default_value: default,
                        }],
                        outputs: vec![],
                    },
                }],
                ..Default::default()
            })
            .unwrap();
        let store = Arc::new(store);
        let backend = Arc::new(FakeBackend::default());
        let runtime = Arc::new(MacroRuntime::new(store.clone(), backend.clone().backends()));
        (directory, store, runtime, backend)
    }
    fn values() -> MkInvocationValues {
        [(MkSignatureId(7), MkValue::String("answer".into()))]
            .into_iter()
            .collect()
    }
    fn wait(runtime: &MacroRuntime, state: RuntimeState) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while runtime.snapshot().state != state {
            assert!(
                Instant::now() < deadline,
                "wanted {state:?}: {:?}",
                runtime.snapshot()
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn token_covers_queue_gui_ownership_drop_and_registration_replacement() {
        let (_directory, _store, runtime, backend) = fixture(None);
        let broker = Arc::new(InvocationPromptBroker::default());
        assert!(
            runtime
                .prepare_invocation(MkInvocation::new(1), &broker)
                .unwrap_err()
                .message
                .contains("GUI")
        );
        assert!(runtime.take_test_commands().is_empty());
        let repaints = Arc::new(AtomicUsize::new(0));
        let registration = broker.register_gui(Arc::new({
            let weak = Arc::downgrade(&broker);
            let repaints = repaints.clone();
            move || {
                // A callback may reenter the broker without deadlocking.
                assert!(weak.upgrade().unwrap().ensure_idle().is_err());
                repaints.fetch_add(1, Ordering::Relaxed);
            }
        }));
        assert!(matches!(
            runtime
                .prepare_invocation(MkInvocation::new(1), &broker)
                .unwrap(),
            InvocationDisposition::AwaitingInput { .. }
        ));
        assert!(
            runtime
                .prepare_invocation(MkInvocation::new(1), &broker)
                .is_err()
        );
        let mut old = registration.take_pending().unwrap();
        assert_eq!(repaints.load(Ordering::Relaxed), 1);
        assert!(registration.take_pending().is_none());
        assert!(
            runtime
                .prepare_invocation(MkInvocation::new(1), &broker)
                .is_err()
        );
        let successor = broker.register_gui(Arc::new(|| {}));
        assert!(!old.is_active());
        drop(registration);
        runtime
            .prepare_invocation(MkInvocation::new(1), &broker)
            .unwrap();
        let new = successor.take_pending().unwrap();
        assert!(old.submit(values()).is_err());
        drop(old);
        assert!(new.is_active(), "old lease must not cancel a successor");
        drop(new);
        broker.ensure_idle().unwrap();
        runtime
            .prepare_invocation(MkInvocation::new(1), &broker)
            .unwrap();
        let mut held = successor.take_pending().unwrap();
        successor.unregister();
        assert!(!held.is_active());
        assert!(held.submit(values()).is_err());
        assert!(runtime.take_test_commands().is_empty());
        assert!(backend.events().is_empty());

        // Dropping the active registration is the fallback for hosts/tests
        // that do not call their explicit exit hook.
        let fallback = broker.register_gui(Arc::new(|| {}));
        runtime
            .prepare_invocation(MkInvocation::new(1), &broker)
            .unwrap();
        let held = fallback.take_pending().unwrap();
        drop(fallback);
        assert!(!held.is_active());
        broker.ensure_idle().unwrap();
    }

    #[test]
    fn confirmation_preserves_six_intents_and_transient_values() {
        for mode in [ExecutionMode::Normal, ExecutionMode::Debug] {
            for subset in [
                MkInvocationSubset::Whole,
                MkInvocationSubset::From(20),
                MkInvocationSubset::Selected(vec![20]),
            ] {
                let (_directory, store, runtime, backend) = fixture(None);
                let broker = Arc::new(InvocationPromptBroker::default());
                let registration = broker.register_gui(Arc::new(|| {}));
                let invocation = MkInvocation {
                    mode,
                    subset: subset.clone(),
                    ..MkInvocation::new(1)
                };
                runtime
                    .prepare_invocation(invocation.clone(), &broker)
                    .unwrap();
                let mut pending = registration.take_pending().unwrap();
                assert_eq!(pending.request().invocation, invocation);
                assert_eq!(pending.request().macro_description, "Prompt description");
                assert_eq!(
                    pending.request().parameters[0].description,
                    "Input description"
                );
                assert!(runtime.take_test_commands().is_empty());
                pending.submit(values()).unwrap();
                wait(&runtime, RuntimeState::Completed);
                let commands = runtime.take_test_commands();
                assert_eq!(
                    commands,
                    [RuntimeCommand::Invoke(MkInvocation {
                        arguments: values(),
                        ..invocation
                    })]
                );
                assert_eq!(
                    backend
                        .events()
                        .iter()
                        .filter(|event| *event == "text:answer")
                        .count(),
                    if subset == MkInvocationSubset::Whole {
                        2
                    } else {
                        1
                    }
                );
                assert_eq!(
                    store.snapshot().macros[0].signature.parameters[0].default_value,
                    None
                );
                assert!(!pending.is_active());
            }
        }
    }

    #[test]
    fn defaults_submit_without_gui_and_explicit_types_reject_before_commands() {
        let (_directory, _store, runtime, backend) =
            fixture(Some(MkValue::String("default".into())));
        let broker = Arc::new(InvocationPromptBroker::default());
        let mut bad = MkInvocation::new(1);
        bad.arguments.insert(MkSignatureId(7), MkValue::Number(1.0));
        assert!(runtime.prepare_invocation(bad, &broker).is_err());
        assert!(runtime.take_test_commands().is_empty());
        assert_eq!(
            runtime
                .prepare_invocation(MkInvocation::new(1), &broker)
                .unwrap(),
            InvocationDisposition::Submitted
        );
        wait(&runtime, RuntimeState::Completed);
        assert_eq!(
            backend
                .events()
                .iter()
                .filter(|event| *event == "text:default")
                .count(),
            2
        );
    }

    #[test]
    fn confirmation_revalidates_signature_subset_enablement_and_runtime_lifetime() {
        for mutation in 0..6 {
            let (_directory, store, runtime, backend) = fixture(None);
            let broker = Arc::new(InvocationPromptBroker::default());
            let registration = broker.register_gui(Arc::new(|| {}));
            let invocation = MkInvocation {
                subset: MkInvocationSubset::Selected(vec![20]),
                ..MkInvocation::new(1)
            };
            runtime.prepare_invocation(invocation, &broker).unwrap();
            let mut pending = registration.take_pending().unwrap();
            let mut changed = (*store.snapshot()).clone();
            let target = &mut changed.macros[0];
            match mutation {
                0 => target.signature.parameters[0].value_type = MkValueType::Number,
                1 => {
                    target.signature.parameters[0].default_value =
                        Some(MkValue::String("new default".into()))
                }
                2 => target.signature.parameters[0].id = MkSignatureId(9),
                3 => {
                    target.steps.pop();
                }
                4 => target.enabled = false,
                _ => runtime.shutdown(),
            }
            store.save(changed).unwrap();
            assert!(pending.submit(values()).is_err(), "mutation {mutation}");
            assert!(runtime.take_test_commands().is_empty());
            assert!(backend.events().is_empty());
        }
    }

    #[test]
    fn confirmation_allows_rename_reorder_but_rejects_busy_without_admission() {
        let (_directory, store, runtime, backend) = fixture(None);
        let broker = Arc::new(InvocationPromptBroker::default());
        let registration = broker.register_gui(Arc::new(|| {}));
        runtime
            .prepare_invocation(MkInvocation::new(1), &broker)
            .unwrap();
        let mut pending = registration.take_pending().unwrap();
        let mut changed = (*store.snapshot()).clone();
        let mut blocker = changed.macros[0].clone();
        blocker.id = 2;
        blocker.signature.parameters[0].default_value = Some(MkValue::String("blocker".into()));
        blocker.steps[0].breakpoint = true;
        changed.macros.push(blocker);
        changed.macros[0].name = "Renamed macro".into();
        changed.macros[0].signature.parameters[0].name = "renamed".into();
        changed.macros[0].signature.parameters[0].description = "New description".into();
        for step in &mut changed.macros[0].steps {
            if let MkAction::Text(text) = &mut step.action {
                text.text = "${renamed}".into();
            }
        }
        changed.macros.reverse();
        store.save(changed).unwrap();
        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(2)),
            crate::mkmacro::CommandResult::Accepted
        );
        wait(&runtime, RuntimeState::Paused);
        runtime.take_test_commands();
        assert!(
            pending
                .submit(values())
                .unwrap_err()
                .message
                .contains("already running")
        );
        assert!(runtime.take_test_commands().is_empty());
        assert!(pending.is_active());
        runtime.command(RuntimeCommand::Stop);
        wait(&runtime, RuntimeState::Stopped);
        pending.submit(values()).unwrap();
        wait(&runtime, RuntimeState::Completed);
        assert_eq!(
            backend
                .events()
                .iter()
                .filter(|event| *event == "text:answer")
                .count(),
            2
        );
    }
}
