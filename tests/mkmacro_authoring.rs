use multi_launcher::{
    actions::Action,
    gui::mkmacro_dialog::{
        MkMacroAuthoringContext, MkMacroDialog, action_catalog,
        action_editor::ActionEditorState,
        launcher_action_picker::PickerPurpose,
        recorder_controller::{
            RecorderController, RecorderControllerView, RecorderState, RecorderStatusSnapshot,
        },
    },
    mkmacro::{executor::fake::FakeBackend, *},
};

#[test]
fn launcher_snapshot_picker_is_transactional_convertible_and_stale_safe() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let actions = Arc::new(vec![
        Action {
            label: "Terminal".into(),
            desc: "display only".into(),
            action: "wt.exe".into(),
            args: Some("--title \"Two Words\"".into()),
        },
        Action {
            label: "Notes".into(),
            desc: "internal".into(),
            action: "notes:dialog".into(),
            args: None,
        },
    ]);
    let mut dialog = MkMacroDialog::new_with_authoring_context(
        Arc::new(store),
        MkMacroAuthoringContext {
            launcher_actions: actions.clone(),
        },
    );
    assert!(Arc::ptr_eq(
        &dialog.authoring_context.launcher_actions,
        &actions
    ));
    dialog.create_macro();
    let macro_id = dialog.selected_macro_id.unwrap();
    dialog
        .action_editor
        .begin_new(MkAction::Process(MkProcessPayload {
            program: String::new(),
            arguments: vec![],
            working_directory: None,
            wait: false,
        }));
    let request = dialog
        .action_editor
        .launcher_picker_request(PickerPurpose::Process, macro_id);
    assert!(dialog.action_editor.apply_launcher_picker_action(
        &request,
        &actions[0],
        Some(macro_id),
        false
    ));
    assert!(
        dialog.selected_macro().unwrap().steps.is_empty(),
        "picker edits only the modal draft"
    );
    dialog.action_editor.cancel();
    assert!(dialog.selected_macro().unwrap().steps.is_empty());

    dialog
        .action_editor
        .begin_new(MkAction::Process(MkProcessPayload {
            program: String::new(),
            arguments: vec![],
            working_directory: None,
            wait: false,
        }));
    let conversion = dialog
        .action_editor
        .launcher_picker_request(PickerPurpose::Process, macro_id);
    assert!(dialog.action_editor.apply_launcher_picker_action(
        &conversion,
        &actions[1],
        Some(macro_id),
        true
    ));
    assert!(
        matches!(dialog.action_editor.draft.as_ref().unwrap().action, MkAction::LauncherCommand { ref command, .. } if command == "notes:dialog")
    );
    dialog.action_editor.cancel();
    dialog
        .action_editor
        .begin_new(MkAction::Delay { milliseconds: 1 });
    assert!(
        !dialog.action_editor.apply_launcher_picker_action(
            &conversion,
            &actions[0],
            Some(macro_id),
            false
        ),
        "an earlier draft request is stale"
    );
}
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn wait_for(runtime: &MacroRuntime, wanted: RuntimeState) -> RuntimeSnapshot {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = runtime.snapshot();
        if snapshot.state == wanted {
            return (*snapshot).clone();
        }
        assert!(
            Instant::now() < deadline,
            "runtime remained {:?} at step {:?}, wanted {:?}; states: {:?}",
            snapshot.state,
            snapshot.step_id,
            wanted,
            snapshot.steps
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for_successful_step(runtime: &MacroRuntime) -> RuntimeSnapshot {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = runtime.snapshot();
        if snapshot
            .steps
            .values()
            .any(|state| *state == StepState::Success)
        {
            return (*snapshot).clone();
        }
        assert!(
            Instant::now() < deadline,
            "runtime reached {:?} at step {:?} without completing a step; states: {:?}",
            snapshot.state,
            snapshot.step_id,
            snapshot.steps
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
    // `Running` is published before the worker can finish its first step. Wait
    // for observable progress so Stop cannot win that scheduling race and
    // leave every step Pending/Running.
    wait_for_successful_step(&runtime);
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
            RecordingBoundary::Event(
                HookEvent::Key {
                    transition: KeyTransition::Down,
                    vk: 65,
                    scan_code: 30,
                    flags: 0,
                    extra_info: 0,
                    timestamp_us: 1,
                },
                None,
            ),
            RecordingBoundary::Event(
                HookEvent::Key {
                    transition: KeyTransition::Up,
                    vk: 65,
                    scan_code: 30,
                    flags: 0,
                    extra_info: 0,
                    timestamp_us: 2,
                },
                None,
            ),
            RecordingBoundary::Event(
                HookEvent::Mouse {
                    message: MouseMessage::Down(MouseButton::Left),
                    x: 10,
                    y: 20,
                    flags: 0,
                    extra_info: 0,
                    timestamp_us: 3,
                },
                None,
            ),
            RecordingBoundary::Event(
                HookEvent::Mouse {
                    message: MouseMessage::Up(MouseButton::Left),
                    x: 10,
                    y: 20,
                    flags: 0,
                    extra_info: 0,
                    timestamp_us: 4,
                },
                None,
            ),
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
    // Rerun the transaction we just edited through the same compiled executor
    // boundary used by MacroRuntime. The asynchronous command/state lifecycle was
    // already exercised above; using the synchronous executor here isolates the
    // recorder -> edit -> save -> playback contract from unrelated worker timing.
    let mut rerun_macro = dialog.selected_macro().unwrap().clone();
    rerun_macro.steps.retain(|step| step.id == click);
    let rerun_plan = compile(&rerun_macro).unwrap();
    let rerun_control = Arc::new(RunControl::default());
    rerun_control.reset();
    let rerun_states = std::sync::Mutex::new(Vec::new());
    Executor::new(fake.clone().backends(), rerun_control)
        .execute(&rerun_plan, &|event| {
            rerun_states.lock().unwrap().push(event)
        })
        .unwrap();
    let rerun_states = rerun_states.into_inner().unwrap();
    assert!(matches!(
        rerun_states.as_slice(),
        [ExecutionEvent::StepStarted(id), ExecutionEvent::StepFinished(done)]
            if id == done && *id == click
    ));
    assert_eq!(
        fake.events()
            .iter()
            .filter(|event| event.as_str() == "button_down:Left")
            .count(),
        2,
        "the transactionally edited double-click must be executed"
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
    assert_eq!(
        action_catalog::action_name(&action),
        "UI Automation — currently unavailable"
    );
    assert_eq!(
        action_catalog::action_details(&action),
        "Unavailable UI Automation action (saved target preserved)"
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
