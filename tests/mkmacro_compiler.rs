use multi_launcher::mkmacro::*;

fn step(id: u64, action: MkAction) -> MkStep {
    MkStep {
        metadata: Default::default(),
        id,
        enabled: true,
        breakpoint: false,
        repeat: 1,
        delay_after_ms: 0,
        on_error: Default::default(),
        action,
    }
}
fn mac(steps: Vec<MkStep>) -> MkMacro {
    MkMacro {
        signature: Default::default(),
        id: 9,
        name: "draft".into(),
        description: String::new(),
        enabled: true,
        hotkey: None,
        hotkey_scope: Default::default(),
        folder_id: None,
        playback: Default::default(),
        steps,
    }
}

#[test]
fn malformed_control_flow_cannot_compile() {
    let invalid = mac(vec![
        step(1, MkAction::Else),
        step(
            2,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 1,
                ..Default::default()
            }),
        ),
    ]);
    let diagnostics = compile(&invalid).unwrap_err();
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Fatal)
    );
}

#[test]
fn compiler_preserves_id_addressing() {
    let plan = compile(&mac(vec![step(
        41,
        MkAction::Delay(MkDelayPayload {
            fixed_ms: 0,
            ..Default::default()
        }),
    )]))
    .unwrap();
    assert_eq!(plan.step_to_instruction[&41], 0);
    assert_eq!(plan.instructions[0].step.id, 41);
}

#[test]
fn hotkey_scope_does_not_change_compiler_admission_or_plan() {
    let mut macro_ = mac(vec![step(41, MkAction::KeyPress(MkKey::Enter))]);
    let unscoped = compile(&macro_).unwrap();
    macro_.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
        process: Some("firefox.exe".into()),
        ..Default::default()
    });
    let scoped = compile(&macro_).unwrap();
    assert_eq!(scoped.macro_id, unscoped.macro_id);
    assert_eq!(scoped.playback, unscoped.playback);
    assert_eq!(scoped.step_to_instruction, unscoped.step_to_instruction);
    assert_eq!(scoped.instructions.len(), unscoped.instructions.len());
    for (actual, expected) in scoped.instructions.iter().zip(unscoped.instructions.iter()) {
        assert_eq!(actual.step, expected.step);
        assert_eq!(actual.jump, expected.jump);
        assert_eq!(actual.depth, expected.depth);
    }
}

#[test]
fn compiled_program_keeps_exact_transitive_closure_and_per_macro_identity() {
    let mut root = mac(vec![step(
        7,
        MkAction::CallMacro(MkCallMacroPayload {
            macro_id: 20,
            ..Default::default()
        }),
    )]);
    root.id = 10;
    root.name = "Root".into();
    root.playback.speed_percent = 125;
    root.steps[0].breakpoint = true;

    let mut child = mac(vec![
        step(
            7,
            MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 30,
                ..Default::default()
            }),
        ),
        step(8, MkAction::Return(Default::default())),
    ]);
    child.id = 20;
    child.name = "Child".into();
    child.playback.random_delay_ms = 9;
    child.steps[0].breakpoint = true;

    let mut leaf = mac(vec![step(7, MkAction::Return(Default::default()))]);
    leaf.id = 30;
    leaf.name = "Leaf".into();
    let mut unrelated = mac(vec![step(1, MkAction::Else)]);
    unrelated.id = 40;

    let document = MkMacroDocument {
        macros: vec![unrelated, leaf, root, child],
        ..Default::default()
    };
    let program = compile_program(&document, 10).unwrap();

    assert_eq!(program.root_macro_id, 10);
    assert_eq!(program.macro_ids(), [10, 20, 30]);
    assert!(
        program.plan(40).is_none(),
        "unrelated invalid macros are excluded"
    );
    assert_eq!(program.name(20), Some("Child"));
    assert_eq!(program.plan(10).unwrap().playback.speed_percent, 125);
    assert_eq!(program.plan(20).unwrap().playback.random_delay_ms, 9);
    assert!(matches!(
        program.plan(10).unwrap().instructions[0].step.action,
        MkAction::CallMacro(MkCallMacroPayload { macro_id: 20, .. })
    ));
    assert!(program.plan(10).unwrap().instructions[0].step.breakpoint);
    assert!(program.plan(20).unwrap().instructions[0].step.breakpoint);
    assert_eq!(program.plan(10).unwrap().step_to_instruction[&7], 0);
    assert_eq!(program.plan(20).unwrap().step_to_instruction[&7], 0);
    assert_eq!(program.plan(30).unwrap().step_to_instruction[&7], 0);
}
