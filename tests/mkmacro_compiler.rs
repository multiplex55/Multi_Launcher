use multi_launcher::mkmacro::*;

fn step(id: u64, action: MkAction) -> MkStep {
    MkStep {
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
        id: 9,
        name: "draft".into(),
        description: String::new(),
        enabled: true,
        hotkey: None,
        hotkey_scope: Default::default(),
        folder_id: None,
        playback: Default::default(),
        steps,
        image_assets: vec![],
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
