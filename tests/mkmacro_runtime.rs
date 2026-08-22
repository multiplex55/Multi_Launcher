use multi_launcher::mkmacro::executor::fake::WindowCall;
use multi_launcher::mkmacro::{executor::fake::FakeBackend, *};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::tempdir;
fn s(id: u64, a: MkAction) -> MkStep {
    MkStep {
        id,
        enabled: true,
        repeat: 1,
        delay_after_ms: 0,
        on_error: Default::default(),
        action: a,
    }
}
fn wait(rt: &MacroRuntime, state: RuntimeState) -> RuntimeSnapshot {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        let x = rt.snapshot();
        if x.state == state {
            return x.as_ref().clone();
        }
        assert!(
            Instant::now() < end,
            "state {:?}, wanted {:?}",
            x.state,
            state
        );
        thread::sleep(Duration::from_millis(2))
    }
}

fn run_window_action(
    action: MkAction,
    configure: impl FnOnce(&FakeBackend),
) -> (RuntimeSnapshot, Arc<FakeBackend>) {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    store
        .save(MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 99,
                name: "window routing".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![s(1, action)],
            }],
        })
        .unwrap();
    let fake = Arc::new(FakeBackend::default());
    configure(&fake);
    let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
    assert_eq!(
        runtime.command(RuntimeCommand::Run(99)),
        CommandResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = runtime.snapshot();
        if matches!(
            snapshot.state,
            RuntimeState::Completed | RuntimeState::Failed | RuntimeState::Stopped
        ) {
            return (snapshot.as_ref().clone(), fake);
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

fn matcher() -> MkWindowMatcher {
    MkWindowMatcher {
        process: Some("notepad.exe".into()),
        title: None,
        title_regex: Some("^literal.*$".into()),
        class: Some("Notepad".into()),
    }
}

#[test]
fn window_activate_forwards_complete_payload() {
    let payload = MkWindowPayload {
        matcher: matcher(),
        wait: Some(MkWaitOptions {
            timeout_ms: 321,
            poll_interval_ms: 17,
        }),
    };
    let (_, fake) = run_window_action(MkAction::WindowActivate(payload.clone()), |_| {});
    assert_eq!(
        *fake.window_calls.lock().unwrap(),
        vec![WindowCall::Activate(payload)]
    );
}

#[test]
fn window_close_forwards_complete_matcher() {
    let expected = matcher();
    let (_, fake) = run_window_action(MkAction::WindowClose(expected.clone()), |_| {});
    assert_eq!(
        *fake.window_calls.lock().unwrap(),
        vec![WindowCall::Close(expected)]
    );
}

fn assert_move_resize(payload: MkWindowMoveResizePayload) {
    let (_, fake) = run_window_action(MkAction::WindowMoveResize(payload.clone()), |_| {});
    assert_eq!(
        *fake.window_calls.lock().unwrap(),
        vec![WindowCall::MoveResize(payload)]
    );
}
#[test]
fn window_move_preserves_absent_size() {
    assert_move_resize(MkWindowMoveResizePayload {
        matcher: matcher(),
        x: Some(-20),
        y: Some(30),
        width: None,
        height: None,
    });
}
#[test]
fn window_resize_preserves_absent_position() {
    assert_move_resize(MkWindowMoveResizePayload {
        matcher: matcher(),
        x: None,
        y: None,
        width: Some(800),
        height: Some(600),
    });
}
#[test]
fn window_move_resize_forwards_all_dimensions() {
    assert_move_resize(MkWindowMoveResizePayload {
        matcher: matcher(),
        x: Some(4),
        y: Some(5),
        width: Some(900),
        height: Some(700),
    });
}

fn assert_state(state: MkWindowState) {
    let expected = matcher();
    let (_, fake) = run_window_action(
        MkAction::WindowState {
            matcher: expected.clone(),
            state,
        },
        |_| {},
    );
    assert_eq!(
        *fake.window_calls.lock().unwrap(),
        vec![WindowCall::SetState(expected, state)]
    );
}
#[test]
fn window_minimize_routes_to_set_state() {
    assert_state(MkWindowState::Minimize);
}
#[test]
fn window_maximize_routes_to_set_state() {
    assert_state(MkWindowState::Maximize);
}
#[test]
fn window_restore_routes_to_set_state() {
    assert_state(MkWindowState::Restore);
}

#[test]
fn window_wait_polls_once_after_success_and_preserves_matcher() {
    let expected = matcher();
    let payload = MkWindowPayload {
        matcher: expected.clone(),
        wait: Some(MkWaitOptions {
            timeout_ms: 250,
            poll_interval_ms: 13,
        }),
    };
    let (_, fake) = run_window_action(MkAction::WindowWait(payload), |fake| {
        fake.conditions
            .lock()
            .unwrap()
            .insert("window_exists".into(), true);
    });
    assert_eq!(
        *fake.window_calls.lock().unwrap(),
        vec![WindowCall::Exists(expected)]
    );
}

#[test]
fn refused_activation_diagnostic_is_not_converted_to_success() {
    let diagnostic = ExecutionDiagnostic::new(
        DiagnosticKind::InputRejected,
        "foreground activation refused",
    )
    .context("process", "notepad.exe");
    let (snapshot, fake) = run_window_action(
        MkAction::WindowActivate(MkWindowPayload {
            matcher: matcher(),
            wait: None,
        }),
        |fake| fake.fail("window_activate", diagnostic.clone()),
    );
    assert_eq!(snapshot.state, RuntimeState::Failed);
    let failure = snapshot.latest_failure.unwrap();
    assert_eq!(failure.kind, diagnostic.kind);
    assert_eq!(failure.message, diagnostic.message);
    assert_eq!(failure.context.get("process"), Some(&"notepad.exe".into()));
    assert_eq!(
        failure.context.get("backend_operation"),
        Some(&"window".into())
    );
    assert_eq!(failure.context.get("attempt"), Some(&"1".into()));
    assert_eq!(
        failure.context.get("attempts_exhausted"),
        Some(&"true".into())
    );
    assert_eq!(fake.window_calls.lock().unwrap().len(), 1);
}

#[test]
fn missing_and_ambiguous_diagnostics_propagate_kind_and_context() {
    for (kind, message) in [
        (DiagnosticKind::TargetNotFound, "missing"),
        (DiagnosticKind::AmbiguousTarget, "ambiguous"),
    ] {
        let diagnostic =
            ExecutionDiagnostic::new(kind, message).context("matcher.process", "notepad.exe");
        let (snapshot, _) = run_window_action(MkAction::WindowClose(matcher()), |fake| {
            fake.fail("window_close", diagnostic.clone())
        });
        let failure = snapshot.latest_failure.unwrap();
        assert_eq!(failure.kind, diagnostic.kind);
        assert_eq!(failure.message, diagnostic.message);
        assert_eq!(
            failure.context.get("matcher.process"),
            Some(&"notepad.exe".into())
        );
        assert_eq!(
            failure.context.get("backend_operation"),
            Some(&"window".into())
        );
        assert_eq!(failure.context.get("attempt"), Some(&"1".into()));
        assert_eq!(
            failure.context.get("attempts_exhausted"),
            Some(&"true".into())
        );
    }
}

#[test]
fn window_wait_is_cancellable_without_window_mutation() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    store
        .save(MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 100,
                name: "cancel wait".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![s(
                    1,
                    MkAction::WindowWait(MkWindowPayload {
                        matcher: matcher(),
                        wait: Some(MkWaitOptions {
                            timeout_ms: 60_000,
                            poll_interval_ms: 10,
                        }),
                    }),
                )],
            }],
        })
        .unwrap();
    let fake = Arc::new(FakeBackend::default());
    let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
    runtime.command(RuntimeCommand::Run(100));
    let deadline = Instant::now() + Duration::from_secs(1);
    while fake.window_calls.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    runtime.command(RuntimeCommand::Stop);
    let snapshot = wait(&runtime, RuntimeState::Stopped);
    assert_eq!(
        snapshot.latest_failure.as_ref().unwrap().kind,
        DiagnosticKind::Cancelled
    );
    assert!(
        fake.window_calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| matches!(call, WindowCall::Exists(_)))
    );
}

#[test]
fn window_wait_timeout_has_typed_configuration_and_no_mutations() {
    let expected = matcher();
    let (snapshot, fake) = run_window_action(
        MkAction::WindowWait(MkWindowPayload {
            matcher: expected.clone(),
            wait: Some(MkWaitOptions {
                timeout_ms: 12,
                poll_interval_ms: 3,
            }),
        }),
        |_| {},
    );
    let failure = snapshot.latest_failure.unwrap();
    assert_eq!(failure.kind, DiagnosticKind::Timeout);
    assert_eq!(failure.context["timeout_ms"], "12");
    assert_eq!(failure.context["poll_interval_ms"], "3");
    assert!(
        fake.window_calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| matches!(call, WindowCall::Exists(value) if value == &expected))
    );
}
#[test]
fn fake_backed_end_to_end_has_exact_events_and_row_states() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    let store = Arc::new(store);
    store
        .save(MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 1,
                name: "e2e".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![
                    s(
                        1,
                        MkAction::WindowActivate(MkWindowPayload {
                            matcher: MkWindowMatcher {
                                title: Some("Fake".into()),
                                title_regex: None,
                                process: None,
                                class: None,
                            },
                            wait: None,
                        }),
                    ),
                    s(
                        2,
                        MkAction::MouseMove(MkMouseMovePayload {
                            target: MkCoordinateTarget::Screen {
                                point: MkPoint { x: 10, y: -2 },
                            },
                            duration_ms: 0,
                        }),
                    ),
                    s(
                        3,
                        MkAction::MouseClick(MkMousePayload {
                            target: MkCoordinateTarget::Screen {
                                point: MkPoint { x: 11, y: 12 },
                            },
                            button: MkMouseButton::Left,
                            clicks: 1,
                        }),
                    ),
                    s(4, MkAction::Delay { milliseconds: 1 }),
                    s(
                        5,
                        MkAction::Text(MkTextPayload {
                            text: "hé😀".into(),
                            mode: MkTextMode::Type,
                        }),
                    ),
                    s(
                        6,
                        MkAction::Hotkey(vec![MkKey::Control, MkKey::Character("A".into())]),
                    ),
                    s(
                        7,
                        MkAction::LauncherCommand {
                            command: "help".into(),
                            args: None,
                        },
                    ),
                ],
            }],
        })
        .unwrap();
    let f = Arc::new(FakeBackend::default());
    let rt = MacroRuntime::new(store, f.clone().backends());
    assert_eq!(rt.command(RuntimeCommand::Run(1)), CommandResult::Accepted);
    let snap = wait(&rt, RuntimeState::Completed);
    assert_eq!(
        f.events(),
        vec![
            "window_activate",
            "move:10,-2",
            "move:11,12",
            "button_down:Left",
            "button_up:Left",
            "text:hé😀",
            "key_down:Control",
            "key_down:Character(\"A\")",
            "key_up:Character(\"A\")",
            "key_up:Control",
            "command:help"
        ]
    );
    assert_eq!(
        snap.steps.values().copied().collect::<Vec<_>>(),
        vec![StepState::Success; 7]
    );
}
#[test]
fn stop_during_held_key_wakes_and_cleans_up() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    let store = Arc::new(store);
    store
        .save(MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 2,
                name: "stop".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![
                    s(1, MkAction::KeyDown(MkKey::Control)),
                    s(
                        2,
                        MkAction::Delay {
                            milliseconds: 60_000,
                        },
                    ),
                    s(3, MkAction::KeyPress(MkKey::Enter)),
                ],
            }],
        })
        .unwrap();
    let f = Arc::new(FakeBackend::default());
    let rt = MacroRuntime::new(store, f.clone().backends());
    rt.command(RuntimeCommand::Run(2));
    wait(&rt, RuntimeState::Running);
    let until = Instant::now() + Duration::from_secs(1);
    while f.events().is_empty() {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    let t = Instant::now();
    rt.command(RuntimeCommand::Stop);
    let snap = wait(&rt, RuntimeState::Stopped);
    assert!(t.elapsed() < Duration::from_millis(500));
    assert_eq!(f.events(), vec!["key_down:Control", "key_up:Control"]);
    assert_eq!(snap.steps[&1], StepState::Success);
    assert_eq!(snap.steps[&2], StepState::Failed);
    assert_eq!(snap.steps[&3], StepState::Pending);
}
