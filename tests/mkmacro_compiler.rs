use multi_launcher::mkmacro::*;

fn step(id: u64, action: MkAction) -> MkStep {
    MkStep {
        id,
        enabled: true,
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
        playback: Default::default(),
        steps,
        image_assets: vec![],
    }
}

#[test]
fn malformed_control_flow_cannot_compile() {
    let invalid = mac(vec![
        step(1, MkAction::Else),
        step(2, MkAction::Delay { milliseconds: 1 }),
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
    let plan = compile(&mac(vec![step(41, MkAction::Delay { milliseconds: 0 })])).unwrap();
    assert_eq!(plan.step_to_instruction[&41], 0);
    assert_eq!(plan.instructions[0].step.id, 41);
}
