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
            "state {:?}, wanted {:?}, failure: {:?}",
            x.state,
            state,
            &x.latest_failure,
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
                image_assets: vec![],
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

fn notification_sequence(policy: MkErrorPolicy) -> MkMacro {
    let mut notify = s(
        3,
        MkAction::Notify(MkNotifyPayload {
            title: "Backup".into(),
            description: r"Copied ${files_copied} files to ${destination}".into(),
            kind: MkNotificationKind::Success,
            duration: MkNotificationDuration::Long,
            show_symbol: false,
        }),
    );
    notify.on_error = policy;
    MkMacro {
        id: 700,
        name: "notification sequence".into(),
        description: String::new(),
        enabled: true,
        hotkey: None,
        playback: Default::default(),
        steps: vec![
            s(
                1,
                MkAction::SetVariable {
                    name: "files_copied".into(),
                    value: MkValue::Number(42.0),
                },
            ),
            s(
                2,
                MkAction::SetVariable {
                    name: "destination".into(),
                    value: MkValue::String(r"D:\Backup".into()),
                },
            ),
            notify,
            s(
                4,
                MkAction::PlaySound(MkPlaySoundPayload {
                    sound: "ReminderStart.wav".into(),
                }),
            ),
            s(
                5,
                MkAction::Text(MkTextPayload {
                    text: "final".into(),
                    mode: MkTextMode::Type,
                }),
            ),
        ],
        image_assets: vec![],
    }
}

fn run_notification_sequence(
    policy: MkErrorPolicy,
    fail: bool,
) -> (RuntimeSnapshot, Arc<FakeBackend>) {
    let dir = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let continues = matches!(&policy, MkErrorPolicy::Continue);
    store
        .save(MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            settings: Default::default(),
            macros: vec![notification_sequence(policy)],
        })
        .unwrap();
    drop(store);
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    compile(&store.snapshot().macros[0]).unwrap();
    let fake = Arc::new(FakeBackend::default());
    if fail {
        fake.fail(
            "notification",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "fake notification failure"),
        );
    }
    let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
    assert_eq!(
        runtime.command(RuntimeCommand::Run(700)),
        CommandResult::Accepted
    );
    let terminal = if fail && !continues {
        RuntimeState::Failed
    } else {
        RuntimeState::Completed
    };
    (wait(&runtime, terminal), fake)
}

#[test]
fn reopened_notification_and_sound_execute_silently_in_order() {
    let (snapshot, fake) = run_notification_sequence(MkErrorPolicy::Stop, false);
    assert!(snapshot.latest_failure.is_none());
    let notifications = fake.notifications();
    assert_eq!(notifications.len(), 1);
    assert!(notifications[0].description.contains("42"));
    assert!(notifications[0].description.contains(r"D:\Backup"));
    assert_eq!(fake.sounds(), ["ReminderStart.wav"]);
    assert_eq!(fake.events(), ["notification", "sound", "text:final"]);
}

#[test]
fn notification_error_policies_gate_sound_and_following_action() {
    let (_, stop) = run_notification_sequence(MkErrorPolicy::Stop, true);
    assert_eq!(stop.notifications().len(), 1);
    assert!(stop.sounds().is_empty());
    assert_eq!(stop.events(), ["notification"]);

    let (_, keep_going) = run_notification_sequence(MkErrorPolicy::Continue, true);
    assert_eq!(keep_going.notifications().len(), 1);
    assert_eq!(keep_going.sounds(), ["ReminderStart.wav"]);
    assert_eq!(keep_going.events(), ["notification", "sound", "text:final"]);

    let (_, retry) = run_notification_sequence(
        MkErrorPolicy::Retry(MkRetry {
            attempts: 3,
            delay_ms: 0,
        }),
        true,
    );
    assert_eq!(retry.notifications().len(), 3);
    assert!(retry.sounds().is_empty());
    assert_eq!(
        retry.events(),
        ["notification", "notification", "notification"]
    );
}

#[test]
fn image_find_result_drives_following_mouse_move_without_platform_effects() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    let asset_path = store
        .write_png_asset(
            70,
            10,
            &image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255])),
        )
        .unwrap();
    let image = MkImagePayload {
        asset_id: 10,
        wait: MkWaitOptions {
            timeout_ms: 1,
            poll_interval_ms: 1,
        },
        region: Default::default(),
        tolerance: 0,
        alpha: Default::default(),
        return_point: Default::default(),
        not_found_policy: MkImageNotFoundPolicy::Continue,
        outputs: MkImageOutputs::default(),
    };
    store
        .save(MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            settings: Default::default(),
            macros: vec![MkMacro {
                id: 70,
                name: "image sequence".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![
                    s(1, MkAction::ImageFind(image)),
                    s(
                        2,
                        MkAction::MouseMove(MkMouseMovePayload {
                            target: MkCoordinateTarget::Image {
                                asset_id: 10,
                                offset: MkPoint { x: 4, y: -3 },
                            },
                            duration_ms: 250,
                        }),
                    ),
                    s(
                        3,
                        MkAction::Text(MkTextPayload {
                            text: "after-image".into(),
                            mode: MkTextMode::Type,
                        }),
                    ),
                ],
                image_assets: vec![MkImageAsset {
                    id: 10,
                    name: "fixture".into(),
                    relative_path: asset_path.to_string_lossy().into_owned(),
                }],
            }],
        })
        .unwrap();
    let fake = Arc::new(FakeBackend::default());
    fake.script_image(10, Ok(Some(MkPoint { x: 30, y: 40 })));
    let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
    assert_eq!(
        runtime.command(RuntimeCommand::Run(70)),
        CommandResult::Accepted
    );
    let snapshot = wait(&runtime, RuntimeState::Completed);
    assert!(snapshot.latest_failure.is_none());
    assert!(fake.events().contains(&"smooth_move:34,37:250".into()));
    assert!(fake.events().contains(&"text:after-image".into()));
}

#[test]
fn prompt_request_result_and_following_step_form_one_runtime_transaction() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    store
        .save(MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            settings: Default::default(),
            macros: vec![MkMacro {
                id: 7,
                name: "prompt plumbing".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![
                    s(
                        1,
                        MkAction::PromptInput(MkPromptInputPayload {
                            title: "Title ${seed}".into(),
                            prompt: "Value for ${seed}?".into(),
                            default_value: "$${seed} / ${seed}".into(),
                            variable: "answer".into(),
                            copy_to_clipboard: false,
                        }),
                    ),
                    s(
                        2,
                        MkAction::Text(MkTextPayload {
                            text: "received:${answer}".into(),
                            mode: MkTextMode::Type,
                        }),
                    ),
                ],
                image_assets: vec![],
            }],
        })
        .unwrap();
    // Seed through a preceding SetVariable so interpolation demonstrably occurs at execution time.
    let mut doc = store.snapshot().as_ref().clone();
    doc.macros[0].steps.insert(
        0,
        s(
            3,
            MkAction::SetVariable {
                name: "seed".into(),
                value: MkValue::String("東京 !".into()),
            },
        ),
    );
    store.save(doc).unwrap();
    let fake = Arc::new(FakeBackend::default());
    fake.script_prompt(PromptResponse::Submitted("accepted ✓".into()));
    let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
    assert_eq!(
        runtime.command(RuntimeCommand::Run(7)),
        CommandResult::Accepted
    );
    let snapshot = wait(&runtime, RuntimeState::Completed);
    assert!(snapshot.latest_failure.is_none());
    let prompts = fake.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].title, "Title 東京 !");
    assert_eq!(prompts[0].prompt, "Value for 東京 !?");
    assert_eq!(
        prompts[0].default_value, "${seed} / 東京 !",
        "escaped placeholders stay literal and expansion is nonrecursive"
    );
    drop(prompts);
    assert!(
        fake.events()
            .contains(&"text:received:accepted ✓".to_string()),
        "the prompt answer is immediately consumable"
    );
}

#[test]
fn cancelled_prompt_honors_stop_and_has_no_later_side_effect() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    store
        .save(MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            settings: Default::default(),
            macros: vec![MkMacro {
                id: 8,
                name: "cancel".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![
                    s(1, MkAction::PromptInput(MkPromptInputPayload::default())),
                    s(
                        2,
                        MkAction::Text(MkTextPayload {
                            text: "must-not-run".into(),
                            mode: MkTextMode::Type,
                        }),
                    ),
                ],
                image_assets: vec![],
            }],
        })
        .unwrap();
    let fake = Arc::new(FakeBackend::default());
    fake.script_prompt(PromptResponse::Cancelled);
    let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
    runtime.command(RuntimeCommand::Run(8));
    let snapshot = wait(&runtime, RuntimeState::Stopped);
    let failure = snapshot
        .latest_failure
        .expect("prompt cancellation should retain its step diagnostic");
    assert_eq!(failure.kind, DiagnosticKind::Cancelled);
    assert!(failure.message.contains("cancelled"));
    assert!(
        !fake
            .events()
            .iter()
            .any(|event| event == "text:must-not-run")
    );
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
                image_assets: vec![],
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
                        MkAction::LauncherCommand(MkLauncherCommandPayload {
                            query: "help".into(),
                            legacy_resolved_action: None,
                        }),
                    ),
                ],
                image_assets: vec![],
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
                image_assets: vec![],
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
