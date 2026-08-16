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
        RecordingBoundary::Event(key(0, 0)),
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
            &[RecordingBoundary::Event(key(0, 0))],
            &NormalizationConfig::default(),
            None
        )
        .len(),
        1
    );
}

fn mouse(timestamp_us: u64, message: MouseMessage, x: i32, y: i32) -> RecordingBoundary {
    RecordingBoundary::Event(HookEvent::Mouse {
        timestamp_us,
        message,
        x,
        y,
        flags: 0,
        extra_info: 0,
    })
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
    let steps = to_macro_steps(&normalized, 100);
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
