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
fn virtual_desktop_catalog_entries_are_searchable_typed_and_stable() {
    let expected = [
        (
            "Create Virtual Desktop",
            MkVirtualDesktopAction::Create,
            "create",
            "Create a new virtual desktop",
        ),
        (
            "Switch Virtual Desktop Left",
            MkVirtualDesktopAction::SwitchLeft,
            "previous",
            "Switch virtual desktop left",
        ),
        (
            "Switch Virtual Desktop Right",
            MkVirtualDesktopAction::SwitchRight,
            "next",
            "Switch virtual desktop right",
        ),
        (
            "Close Current Virtual Desktop",
            MkVirtualDesktopAction::CloseCurrent,
            "close",
            "Close the current virtual desktop using native Windows behavior",
        ),
    ];
    let visible: Vec<_> = action_catalog::visible_descriptors().collect();
    for (name, operation, query, summary) in expected {
        let descriptor = visible.iter().find(|entry| entry.name == name).expect(name);
        assert!(action_catalog::matches(descriptor, query));
        let action = (descriptor.make_default)();
        assert_eq!(action, MkAction::VirtualDesktop(operation));
        assert_eq!(action_catalog::action_name(&action), name);
        assert_eq!(action_catalog::action_details(&action), summary);
        assert_eq!(descriptor.category, action_catalog::ActionCategory::Windows);
        assert_eq!(descriptor.editor, action_catalog::EditorKind::DirectInsert);
    }
}

#[test]
fn catalog_is_a_complete_bidirectional_capability_contract() {
    use action_catalog::{
        ActionAvailability, DraftValidationContract, EditorKind, RuntimeAvailability,
    };

    let all = action_catalog::descriptors();
    for descriptor in &all {
        let action =
            std::panic::catch_unwind(|| (descriptor.make_default)()).unwrap_or_else(|_| {
                panic!("{}: default action construction panicked", descriptor.name)
            });
        assert!(
            !descriptor.name.trim().is_empty(),
            "catalog action has an empty name"
        );
        assert!(
            !action_catalog::action_name(&action).trim().is_empty(),
            "{} has no stable action_name",
            descriptor.name
        );
        assert!(
            !action_catalog::action_details(&action).trim().is_empty(),
            "{} has no stable action_details",
            descriptor.name
        );

        match descriptor.availability {
            ActionAvailability::Ready => {
                assert_eq!(
                    descriptor.hidden_reason, None,
                    "{} is Ready but has a hidden reason",
                    descriptor.name
                );
                assert_eq!(
                    descriptor.runtime,
                    RuntimeAvailability::Supported,
                    "{} was marked Ready without runtime support; implement both runtime and editor paths and their tests, or hide the entry with a concrete reason",
                    descriptor.name
                );
                assert!(
                    executor::has_runtime_support(&action),
                    "{} was marked Ready without has_runtime_support; implement both runtime and editor paths and their tests, or hide the entry with a concrete reason",
                    descriptor.name
                );
                let contract = descriptor.editor.contract().unwrap_or_else(|| panic!(
                    "{} was marked Ready without EditorContract; implement both runtime and editor paths and their tests, or hide the entry with a concrete reason", descriptor.name));
                assert!(
                    action_catalog::editor_route_recognizes(&action, descriptor.editor),
                    "{} editor/action route drifted",
                    descriptor.name
                );
                if descriptor.editor == EditorKind::DirectInsert {
                    assert!(
                        action_catalog::requires_no_configuration(&action),
                        "{} is not an intentional structural DirectInsert",
                        descriptor.name
                    );
                } else {
                    assert!(
                        matches!(contract, action_catalog::EditorContract::Configurable { field_count } if field_count > 0)
                    );
                }
                if action_catalog::draft_validation_contract(&action)
                    != DraftValidationContract::CommitReady
                {
                    assert_eq!(
                        action_catalog::draft_validation_contract(&action),
                        DraftValidationContract::AwaitingRequiredAsset,
                        "{} must explain the missing required configuration",
                        descriptor.name
                    );
                }
            }
            ActionAvailability::Hidden => {
                assert!(
                    !descriptor
                        .hidden_reason
                        .unwrap_or_default()
                        .trim()
                        .is_empty(),
                    "{} lacks a product/capability hidden_reason",
                    descriptor.name
                );
                assert!(!action_catalog::is_available_in_palette(descriptor));
                assert!(
                    !action_catalog::visible_descriptors()
                        .any(|visible| visible.name == descriptor.name)
                );
                // Matching a descriptor is deliberately separate from palette filtering:
                // exact names and every advertised keyword still cannot make a hidden row selectable.
                assert!(action_catalog::matches(descriptor, descriptor.name));
                for keyword in descriptor.keywords {
                    assert!(
                        !action_catalog::visible_descriptors()
                            .any(|visible| action_catalog::matches(&visible, keyword)
                                && visible.name == descriptor.name)
                    );
                }
            }
        }
    }
}

#[test]
fn catalog_window_desktop_prompt_and_image_inventory_is_intentional() {
    let visible: Vec<_> = action_catalog::visible_descriptors().collect();
    let names = |category| {
        visible
            .iter()
            .filter(|d| d.category == category)
            .map(|d| d.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(action_catalog::ActionCategory::Windows),
        vec![
            "Activate Window",
            "Close Window",
            "Wait for Window",
            "Move / Resize Window",
            "Minimize Window",
            "Create Virtual Desktop",
            "Switch Virtual Desktop Left",
            "Switch Virtual Desktop Right",
            "Close Current Virtual Desktop",
            "Maximize Window",
            "Restore Window",
        ]
    );
    for expected in ["Prompt for Input", "Find Image", "Click Image"] {
        assert_eq!(
            visible.iter().filter(|d| d.name == expected).count(),
            1,
            "catalog inventory drift for {expected}"
        );
    }
}

#[test]
fn prompt_input_authoring_apply_and_cancel_are_transactional() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let mut dialog = MkMacroDialog::new(Arc::new(store));
    dialog.create_macro();
    let payload = MkPromptInputPayload {
        title: "Customer label".into(),
        prompt: "Enter a value".into(),
        default_value: "Unicode ✓ default".into(),
        variable: "answer_2".into(),
        copy_to_clipboard: true,
    };
    dialog.action_editor.begin_new_with_editor(
        MkAction::PromptInput(payload.clone()),
        action_catalog::EditorKind::PromptInput,
    );
    assert_eq!(
        dialog.action_editor.editor,
        Some(action_catalog::EditorKind::PromptInput)
    );
    let mut editor = dialog.take_action_editor();
    editor
        .apply(&mut dialog)
        .expect("valid prompt draft applies");
    dialog.action_editor = editor;
    assert_eq!(
        dialog.selected_macro().unwrap().steps[0].action,
        MkAction::PromptInput(payload.clone())
    );

    let saved = dialog.selected_macro().unwrap().steps[0].clone();
    dialog.action_editor.begin_edit(&saved);
    if let MkAction::PromptInput(draft) = &mut dialog.action_editor.draft.as_mut().unwrap().action {
        draft.prompt = "discard me".into();
    }
    dialog.action_editor.cancel();
    assert_eq!(dialog.selected_macro().unwrap().steps[0], saved);
}

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

#[test]
fn deleting_last_macro_clears_editor_requests_and_persists_empty_document() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = MkMacroStore::open(dir.path()).unwrap();
    let store = Arc::new(store);
    let mut dialog = MkMacroDialog::new(Arc::clone(&store));
    dialog.create_macro();
    assert!(dialog.rename_selected("Delete this final macro"));
    insert(&mut dialog, MkAction::Delay { milliseconds: 9 });
    dialog.save().unwrap();
    let deleted_id = dialog.selected_macro_id.unwrap();

    dialog
        .action_editor
        .begin_new(MkAction::Delay { milliseconds: 10 });
    let request = dialog
        .action_editor
        .launcher_picker_request(PickerPurpose::LauncherCommand, deleted_id);
    dialog.launcher_action_picker.open(request);
    assert!(dialog.action_editor.draft.is_some());
    assert!(dialog.launcher_action_picker.request.is_some());

    dialog.delete_selected_macro();
    assert!(dialog.draft.macros.is_empty());
    assert_eq!(dialog.selected_macro_id, None);
    assert!(dialog.selection.ids.is_empty());
    assert!(dialog.action_editor.draft.is_none());
    assert!(dialog.action_editor.insertion.is_none());
    assert!(dialog.launcher_action_picker.request.is_none());
    assert!(!dialog.launcher_action_picker.open);

    dialog.save().unwrap();
    assert!(store.snapshot().macros.is_empty());
    drop(dialog);
    drop(store);

    let (reopened, _) = MkMacroStore::open(dir.path()).unwrap();
    assert!(reopened.snapshot().macros.is_empty());
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
    let mut editor = ActionEditorState::new(dialog.visual_overlay_controller());
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
    let mut editor = ActionEditorState::new(dialog.visual_overlay_controller());
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
        image_assets: vec![],
    })
    .unwrap();
    let error = Executor::new(Backends::unsupported(), Arc::new(RunControl::default()))
        .execute(&plan, &|_| {})
        .unwrap_err();
    assert_eq!(error.message, "UI Automation backend is not available yet");
}
