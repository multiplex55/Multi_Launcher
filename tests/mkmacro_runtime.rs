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
#[test]
fn fake_backed_end_to_end_has_exact_events_and_row_states() {
    let d = tempdir().unwrap();
    let (store, _) = MkMacroStore::open(d.path()).unwrap();
    let store = Arc::new(store);
    store
        .save(MkMacroDocument {
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
                        MkAction::MouseMove(MkCoordinateTarget::Screen {
                            point: MkPoint { x: 10, y: -2 },
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
