use multi_launcher::mkmacro::*;

fn key(extra_info: usize, flags: u32) -> HookEvent {
    HookEvent::Key {
        transition: KeyTransition::Down,
        vk: 65,
        scan_code: 30,
        flags,
        extra_info,
        timestamp_us: 1,
    }
}

#[test]
fn playback_emergency_and_controller_feedback_are_excluded() {
    let playback = key(multi_launcher::mkmacro::input::MKMACRO_EXTRA_INFO, 0);
    let emergency = key(0, LLKHF_INJECTED);
    assert!(
        !should_record(&playback, true),
        "the playback marker always wins"
    );
    assert!(
        !should_record(&emergency, false),
        "injected emergency/controller input is excluded"
    );
    let boundaries = [
        RecordingBoundary::Pause { timestamp_us: 0 },
        RecordingBoundary::Event(key(0, 0), None),
        RecordingBoundary::Resume { timestamp_us: 2 },
    ];
    assert!(
        normalize(&boundaries, &NormalizationConfig::default(), None).is_empty(),
        "record controls are ignored while paused"
    );
}

#[test]
fn ordinary_physical_input_remains_recordable() {
    assert!(should_record(&key(0, 0), false));
    assert_eq!(
        normalize(
            &[RecordingBoundary::Event(key(0, 0), None)],
            &NormalizationConfig::default(),
            None
        )
        .len(),
        1
    );
}

fn mouse(timestamp_us: u64, message: MouseMessage, x: i32, y: i32) -> RecordingBoundary {
    RecordingBoundary::Event(
        HookEvent::Mouse {
            timestamp_us,
            message,
            x,
            y,
            flags: 0,
            extra_info: 0,
        },
        None,
    )
}

#[test]
fn normalized_timed_move_and_drag_keep_final_row_count_and_stable_ids() {
    let mut cfg = NormalizationConfig::default();
    cfg.movement_mode = MovementMode::DetailedMovement;
    let normalized = normalize(
        &[
            mouse(0, MouseMessage::Move, 1, 1),
            mouse(25_000, MouseMessage::Move, 20, 20),
            mouse(40_000, MouseMessage::Down(MouseButton::Right), 20, 20),
            mouse(70_000, MouseMessage::Move, 80, 80),
            mouse(90_000, MouseMessage::Up(MouseButton::Right), 80, 80),
            mouse(102_000, MouseMessage::Wheel(120), 80, 80),
        ],
        &cfg,
        None,
    );
    let steps = to_macro_steps(&normalized, 100, false);
    assert_eq!(steps.len(), 4, "in-drag hook movement must not create rows");
    assert_eq!(
        steps.iter().map(|step| step.id).collect::<Vec<_>>(),
        [101, 102, 103, 104]
    );
    assert!(matches!(
        steps[1].action,
        MkAction::MouseMove(MkMouseMovePayload {
            duration_ms: 25,
            ..
        })
    ));
    assert_eq!(steps[1].delay_after_ms, 15);
    let MkAction::MouseDrag(drag) = &steps[2].action else {
        panic!("drag must remain one direct macro row")
    };
    assert_eq!(drag.button, MkMouseButton::Right);
    assert_eq!(drag.duration_ms, 50);
    assert_eq!(
        drag.from,
        MkCoordinateTarget::Screen {
            point: MkPoint { x: 20, y: 20 }
        }
    );
    assert_eq!(
        drag.to,
        MkCoordinateTarget::Screen {
            point: MkPoint { x: 80, y: 80 }
        }
    );
    assert_eq!(steps[2].delay_after_ms, 12);
}

fn explorer() -> WindowContext {
    WindowContext {
        executable: "explorer.exe".into(),
        title: "Files".into(),
        class: "Cabinet".into(),
        client_origin: Some(MkPoint { x: 100, y: 200 }),
        native_root_id: Some(11),
        ..Default::default()
    }
}
fn notepad() -> WindowContext {
    WindowContext {
        executable: "notepad.exe".into(),
        title: "Notes".into(),
        class: "Notepad".into(),
        client_origin: Some(MkPoint { x: -500, y: 40 }),
        native_root_id: Some(22),
        ..Default::default()
    }
}
fn context(foreground: WindowContext, under: Option<WindowContext>) -> Option<EventContext> {
    Some(EventContext {
        foreground,
        window_under_point: under,
    })
}
fn event(event: HookEvent, context: Option<EventContext>) -> RecordingBoundary {
    RecordingBoundary::Event(event, context)
}
fn keyboard(
    t: u64,
    transition: KeyTransition,
    vk: u32,
    c: Option<EventContext>,
) -> RecordingBoundary {
    event(
        HookEvent::Key {
            timestamp_us: t,
            transition,
            vk,
            scan_code: 0,
            flags: 0,
            extra_info: 0,
        },
        c,
    )
}
fn contextual_mouse(
    t: u64,
    message: MouseMessage,
    x: i32,
    y: i32,
    c: Option<EventContext>,
) -> RecordingBoundary {
    event(
        HookEvent::Mouse {
            timestamp_us: t,
            message,
            x,
            y,
            flags: 0,
            extra_info: 0,
        },
        c,
    )
}
fn click(t: u64, x: i32, y: i32, c: Option<EventContext>) -> [RecordingBoundary; 2] {
    [
        contextual_mouse(t, MouseMessage::Down(MouseButton::Left), x, y, c.clone()),
        contextual_mouse(t + 1_000, MouseMessage::Up(MouseButton::Left), x, y, c),
    ]
}

#[test]
fn contextual_sequence_has_exact_activation_coordinates_and_key_transitions() {
    let e = context(explorer(), Some(explorer()));
    let n = context(notepad(), Some(notepad()));
    let mut input = Vec::new();
    input.extend(click(1_000, 145, 260, e.clone()));
    input.push(keyboard(3_000, KeyTransition::Down, 65, e.clone()));
    input.push(keyboard(4_000, KeyTransition::Up, 65, e));
    // Foreground is still Explorer at the pointer event: the pointed-to Notepad wins.
    let pointed_notepad = context(explorer(), Some(notepad()));
    input.extend(click(5_000, -450, 100, pointed_notepad));
    input.push(keyboard(7_000, KeyTransition::Down, 66, n.clone()));
    input.push(keyboard(8_000, KeyTransition::Up, 66, n));
    let rows = to_macro_steps(
        &normalize(&input, &NormalizationConfig::default(), None),
        0,
        true,
    );
    assert_eq!(
        rows.iter().map(|s| s.action.clone()).collect::<Vec<_>>(),
        vec![
            MkAction::WindowActivate(MkWindowPayload {
                matcher: explorer().matcher().unwrap(),
                wait: None
            }),
            MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::WindowClient {
                    matcher: explorer().matcher().unwrap(),
                    point: MkPoint { x: 45, y: 60 }
                },
                button: MkMouseButton::Left,
                clicks: 1
            }),
            MkAction::KeyDown(MkKey::Character("A".into())),
            MkAction::KeyUp(MkKey::Character("A".into())),
            MkAction::WindowActivate(MkWindowPayload {
                matcher: notepad().matcher().unwrap(),
                wait: None
            }),
            MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::WindowClient {
                    matcher: notepad().matcher().unwrap(),
                    point: MkPoint { x: 50, y: 60 }
                },
                button: MkMouseButton::Left,
                clicks: 1
            }),
            MkAction::KeyDown(MkKey::Character("B".into())),
            MkAction::KeyUp(MkKey::Character("B".into())),
        ]
    );
    assert_eq!(
        explorer().matcher().unwrap().process.as_deref(),
        Some("explorer.exe")
    );
    assert!(!format!("{:?}", rows).contains("native_root_id"));
}

#[test]
fn context_precedence_drag_signed_coordinates_fallback_and_activation_suppression() {
    let e = context(notepad(), Some(explorer()));
    let mut input = vec![contextual_mouse(
        1_000,
        MouseMessage::Move,
        145,
        260,
        e.clone(),
    )];
    input.extend(click(2_000, 145, 260, e.clone()));
    input.push(contextual_mouse(
        4_000,
        MouseMessage::Wheel(120),
        145,
        260,
        e.clone(),
    ));
    input.push(contextual_mouse(
        5_000,
        MouseMessage::Down(MouseButton::Left),
        120,
        230,
        e.clone(),
    ));
    input.push(contextual_mouse(
        8_000,
        MouseMessage::Up(MouseButton::Left),
        180,
        290,
        e.clone(),
    ));
    input.push(keyboard(
        9_000,
        KeyTransition::Down,
        65,
        context(notepad(), Some(explorer())),
    ));
    input.extend(click(
        10_000,
        -550,
        20,
        context(explorer(), Some(notepad())),
    ));
    input.extend(click(12_000, 145, 260, e));
    let mut cfg = NormalizationConfig::default();
    cfg.movement_mode = MovementMode::DetailedMovement;
    let rows = to_macro_steps(&normalize(&input, &cfg, None), 0, true);
    let activations = rows
        .iter()
        .filter_map(|s| match &s.action {
            MkAction::WindowActivate(p) => Some(p.matcher.process.clone().unwrap()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(activations, ["explorer.exe", "notepad.exe", "explorer.exe"]);
    let drag = rows
        .iter()
        .find_map(|s| match &s.action {
            MkAction::MouseDrag(p) => Some(p),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        drag.from,
        MkCoordinateTarget::WindowClient {
            matcher: explorer().matcher().unwrap(),
            point: MkPoint { x: 20, y: 30 }
        }
    );
    assert_eq!(
        drag.to,
        MkCoordinateTarget::WindowClient {
            matcher: explorer().matcher().unwrap(),
            point: MkPoint { x: 80, y: 90 }
        }
    );
    let negative = rows.iter().find_map(|s| match &s.action {
        MkAction::MouseClick(p)
            if p.target
                == (MkCoordinateTarget::WindowClient {
                    matcher: notepad().matcher().unwrap(),
                    point: MkPoint { x: -50, y: -20 },
                }) =>
        {
            Some(&p.target)
        }
        _ => None,
    });
    assert!(
        negative.is_some(),
        "signed subtraction must neither clamp nor wrap"
    );
    assert_eq!(
        rows.iter()
            .filter(|s| matches!(s.action, MkAction::WindowActivate(_)))
            .count(),
        3,
        "moves must not add activation noise"
    );

    let mut no_origin = explorer();
    no_origin.client_origin = None;
    let fallback = RecordedStep {
        timestamp_us: 0,
        delay_after_ms: 0,
        action: RecordedAction::Click {
            button: MouseButton::Left,
            x: 7,
            y: -9,
            count: 1,
        },
        context: context(no_origin.clone(), None),
    };
    let fallback_rows = to_macro_steps(&[fallback], 0, true);
    let MkAction::MouseClick(p) = &fallback_rows[1].action else {
        panic!()
    };
    assert_eq!(
        p.target,
        MkCoordinateTarget::Screen {
            point: MkPoint { x: 7, y: -9 }
        }
    );
    assert_eq!(
        fallback_rows[0].action,
        MkAction::WindowActivate(MkWindowPayload {
            matcher: no_origin.matcher().unwrap(),
            wait: None
        }),
        "foreground is the explicit pointer fallback"
    );
}

#[test]
fn context_opt_out_preserves_screen_targets_delays_and_control_hotkey_filtering() {
    let c = context(explorer(), Some(notepad()));
    let input = vec![
        contextual_mouse(0, MouseMessage::Move, -10, 20, c.clone()),
        contextual_mouse(
            10_000,
            MouseMessage::Down(MouseButton::Left),
            -20,
            30,
            c.clone(),
        ),
        contextual_mouse(
            30_000,
            MouseMessage::Up(MouseButton::Left),
            40,
            -50,
            c.clone(),
        ),
        keyboard(40_000, KeyTransition::Down, 65, c.clone()),
        keyboard(41_000, KeyTransition::Down, 0x78, c.clone()),
        keyboard(42_000, KeyTransition::Down, 0x78, c.clone()),
        keyboard(43_000, KeyTransition::Up, 0x78, c.clone()),
        keyboard(44_000, KeyTransition::Down, 66, c),
    ];
    let mut cfg = NormalizationConfig::default();
    cfg.record_window_context = false;
    cfg.movement_mode = MovementMode::DetailedMovement;
    cfg.control_hotkeys = vec![0x78];
    let normalized = normalize(&input, &cfg, None);
    let rows = to_macro_steps(&normalized, 0, false);
    assert!(
        !rows
            .iter()
            .any(|s| matches!(s.action, MkAction::WindowActivate(_)))
    );
    assert!(
        matches!(&rows[0].action, MkAction::MouseMove(p) if p.target == MkCoordinateTarget::Screen { point: MkPoint { x: -10, y: 20 } })
    );
    assert!(
        matches!(&rows[1].action, MkAction::MouseDrag(p) if p.from == MkCoordinateTarget::Screen { point: MkPoint { x: -20, y: 30 } } && p.to == MkCoordinateTarget::Screen { point: MkPoint { x: 40, y: -50 } })
    );
    assert_eq!(
        rows.iter()
            .filter_map(|s| match &s.action {
                MkAction::KeyDown(k) | MkAction::KeyUp(k) | MkAction::KeyPress(k) =>
                    Some(k.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [MkKey::Character("A".into()), MkKey::Character("B".into())]
    );
    assert_eq!(
        normalized[1].delay_after_ms, 10,
        "non-context chronology remains unchanged"
    );
}

struct FakeEnricher {
    calls: usize,
    value: EventContext,
}
impl EventEnricher for FakeEnricher {
    fn enrich(&mut self, _: &HookEvent) -> Option<EventContext> {
        self.calls += 1;
        Some(self.value.clone())
    }
}
#[test]
fn enrichment_is_used_only_when_capture_context_is_missing() {
    let mut fake = FakeEnricher {
        calls: 0,
        value: context(explorer(), Some(notepad())).unwrap(),
    };
    let normalized = normalize(
        &[
            keyboard(1, KeyTransition::Down, 65, None),
            keyboard(
                2,
                KeyTransition::Up,
                65,
                context(explorer(), Some(explorer())),
            ),
        ],
        &NormalizationConfig::default(),
        Some(&mut fake),
    );
    assert_eq!(fake.calls, 1);
    assert_eq!(
        normalized[0].context.as_ref().unwrap().foreground.matcher(),
        notepad().matcher()
    );
    assert_eq!(
        normalized[1].context.as_ref().unwrap().foreground.matcher(),
        explorer().matcher()
    );
}
