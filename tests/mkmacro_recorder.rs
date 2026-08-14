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
