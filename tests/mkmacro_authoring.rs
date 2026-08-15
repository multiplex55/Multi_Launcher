use multi_launcher::{
    gui::mkmacro_dialog::{
        MkMacroDialog, action_catalog,
        action_editor::ActionEditorState,
        recorder_controller::{
            RecorderController, RecorderControllerView, RecorderState, RecorderStatusSnapshot,
        },
    },
    mkmacro::{executor::fake::FakeBackend, *},
};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn wait_for(runtime: &MacroRuntime, wanted: RuntimeState) -> RuntimeSnapshot {
    // The runtime worker is deliberately asynchronous. Windows CI can be heavily
    // oversubscribed, so this is a deadline rather than an assertion about speed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = runtime.snapshot();
        if snapshot.state == wanted {
            return (*snapshot).clone();
        }
        assert!(
            Instant::now() < deadline,
            "runtime remained {:?}",
            snapshot.state
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[derive(Default)]
struct FakeRecorderView;
impl RecorderControllerView for FakeRecorderView {
    fn set_visible(&mut self, _: bool) {}
    fn show(
        &mut self,
        _: &RecorderStatusSnapshot,
        _: Option<&RuntimeSnapshot>,
    ) -> Option<multi_launcher::gui::mkmacro_dialog::recorder_controller::ControllerAction> {
        None
    }
}

fn insert(dialog: &mut MkMacroDialog, action: MkAction) -> u64 {
    let mut editor = ActionEditorState::default();
    editor.begin_new(action);
    editor
        .apply(dialog)
        .expect("GUI editor intent inserts action")
}

#[test]
fn complete_authoring_recording_and_playback_workflow_uses_typed_intents() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let store = Arc::new(store);
    let mut dialog = MkMacroDialog::new(store.clone());
    assert!(dialog.draft.macros.is_empty());
    assert!(validate_document(&dialog.draft, None).is_empty());

    dialog.create_macro();
    assert!(dialog.rename_selected("Notepad Demo"));
    let matcher = MkWindowMatcher {
        title: None,
        title_regex: None,
        process: Some("notepad.exe".into()),
        class: None,
    };
    insert(
        &mut dialog,
        MkAction::Process(MkProcessPayload {
            program: "notepad.exe".into(),
            arguments: vec![],
            working_directory: None,
            wait: false,
        }),
    );
    insert(
        &mut dialog,
        MkAction::WindowWait(MkWindowPayload {
            matcher,
            wait: Some(MkWaitOptions {
                timeout_ms: 500,
                poll_interval_ms: 1,
            }),
        }),
    );
    insert(
        &mut dialog,
        MkAction::Text(MkTextPayload {
            text: "first".into(),
            mode: MkTextMode::Type,
        }),
    );
    insert(
        &mut dialog,
        MkAction::Hotkey(vec![MkKey::Control, MkKey::Character("A".into())]),
    );
    insert(
        &mut dialog,
        MkAction::Text(MkTextPayload {
            text: "replacement".into(),
            mode: MkTextMode::Type,
        }),
    );
    insert(&mut dialog, MkAction::Delay { milliseconds: 250 });

    let visible = dialog
        .selected_macro()
        .unwrap()
        .steps
        .iter()
        .map(|s| {
            (
                action_catalog::action_name(&s.action),
                action_catalog::action_details(&s.action),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        visible.iter().map(|x| x.0).collect::<Vec<_>>(),
        vec![
            "Run Program",
            "Wait for Window",
            "Text",
            "Hotkey",
            "Text",
            "Delay"
        ]
    );
    dialog.save().unwrap();
    assert_eq!(&*store.snapshot(), &dialog.draft);

    let fake = Arc::new(FakeBackend::default());
    fake.conditions
        .lock()
        .unwrap()
        .insert("window_exists".into(), true);
    let runtime = MacroRuntime::new(store.clone(), fake.clone().backends());
    let id = dialog.selected_macro_id.unwrap();
    assert_eq!(
        runtime.command(RuntimeCommand::Run(id)),
        CommandResult::Accepted
    );
    wait_for(&runtime, RuntimeState::Running);
    assert_eq!(
        runtime.command(RuntimeCommand::Pause),
        CommandResult::Accepted
    );
    wait_for(&runtime, RuntimeState::Paused);
    assert_eq!(
        runtime.command(RuntimeCommand::Resume),
        CommandResult::Accepted
    );
    wait_for(&runtime, RuntimeState::Running);
    assert_eq!(
        runtime.command(RuntimeCommand::Stop),
        CommandResult::Accepted
    );
    let stopped = wait_for(&runtime, RuntimeState::Stopped);
    assert!(stopped.steps.values().any(|s| *s == StepState::Success));

    let mut recorder = RecorderController::new(FakeRecorderView);
    recorder.hook_command(HookCommand::Start);
    assert_eq!(recorder.status.state, RecorderState::Recording);
    let recorded = normalize(
        &[
            RecordingBoundary::Event(HookEvent::Key {
                transition: KeyTransition::Down,
                vk: 65,
                scan_code: 30,
                flags: 0,
                extra_info: 0,
                timestamp_us: 1,
            }),
            RecordingBoundary::Event(HookEvent::Mouse {
                message: MouseMessage::Down(MouseButton::Left),
                x: 10,
                y: 20,
                flags: 0,
                extra_info: 0,
                timestamp_us: 2,
            }),
            RecordingBoundary::Event(HookEvent::Mouse {
                message: MouseMessage::Up(MouseButton::Left),
                x: 10,
                y: 20,
                flags: 0,
                extra_info: 0,
                timestamp_us: 3,
            }),
        ],
        &NormalizationConfig {
            movement_mode: MovementMode::ClicksOnly,
            ..Default::default()
        },
        None,
    );
    recorder.hook_command(HookCommand::Stop);
    assert_eq!(recorder.status.state, RecorderState::Stopped);
    let before = dialog.draft.clone();
    assert!(dialog.apply_recording(u64::MAX, &recorded).is_err());
    assert_eq!(dialog.draft, before, "recorder failure is atomic");
    let ids = dialog.apply_recording(id, &recorded).unwrap();
    let click = *ids.last().unwrap();
    let original = dialog
        .selected_macro()
        .unwrap()
        .steps
        .iter()
        .find(|s| s.id == click)
        .unwrap()
        .clone();
    let mut editor = ActionEditorState::default();
    editor.begin_edit(&original);
    if let Some(step) = &mut editor.draft {
        if let MkAction::MouseClick(payload) = &mut step.action {
            payload.clicks = 2;
        } else {
            panic!("normalized click missing")
        }
    }
    assert_eq!(editor.apply(&mut dialog), Some(click));
    dialog.save().unwrap();
    assert_eq!(&*store.snapshot(), &dialog.draft);
    assert_eq!(
        runtime.command(RuntimeCommand::Run(id)),
        CommandResult::Accepted
    );
    assert_eq!(
        wait_for(&runtime, RuntimeState::Completed).state,
        RuntimeState::Completed
    );
}

#[test]
fn serialized_uia_remains_presentable_and_reports_missing_backend() {
    let payload = MkUiPayload {
        window: MkWindowMatcher {
            title: None,
            title_regex: None,
            process: Some("notepad.exe".into()),
            class: None,
        },
        selector: MkUiSelector {
            automation_id: Some("editor".into()),
            name: None,
            control_type: None,
            class_name: None,
            framework_id: None,
            ancestor_path: vec![],
        },
        wait: None,
    };
    let action: MkAction =
        serde_json::from_value(serde_json::to_value(MkAction::UiInvoke(payload)).unwrap()).unwrap();
    assert_eq!(action_catalog::action_name(&action), "Invoke UI Element");
    assert_eq!(
        action_catalog::action_details(&action),
        "UI Automation target"
    );
    let plan = compile(&MkMacro {
        id: 1,
        name: "uia".into(),
        description: String::new(),
        enabled: true,
        hotkey: None,
        playback: Default::default(),
        steps: vec![MkStep {
            id: 1,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        }],
    })
    .unwrap();
    let error = Executor::new(Backends::unsupported(), Arc::new(RunControl::default()))
        .execute(&plan, &|_| {})
        .unwrap_err();
    assert_eq!(error.message, "UI Automation backend is not available yet");
}
