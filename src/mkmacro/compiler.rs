use super::{model::*, validation::*};
use std::collections::HashMap;
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq)]
pub enum Jump {
    Next,
    To(usize),
    IfFalse(usize),
    RepeatBegin { exit: usize },
    RepeatEnd { start: usize, exit: usize },
    WhileEnd { condition: usize },
    Break(usize),
    Continue(usize),
}
#[derive(Debug, Clone)]
pub struct MkInstruction {
    pub step: Arc<MkStep>,
    pub depth: usize,
    pub jump: Jump,
}
#[derive(Debug, Clone)]
pub struct MkExecutionPlan {
    pub macro_id: u64,
    pub name: String,
    pub enabled: bool,
    pub signature: MkCompiledSignature,
    pub playback: MkPlayback,
    pub instructions: Arc<[MkInstruction]>,
    pub step_to_instruction: HashMap<u64, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MkCompiledSignature {
    parameters: Arc<[MkMacroParameter]>,
    outputs: Arc<[MkMacroOutput]>,
    parameter_indices: HashMap<MkSignatureId, usize>,
    output_indices: HashMap<MkSignatureId, usize>,
}
impl MkCompiledSignature {
    fn new(signature: &MkMacroSignature) -> Self {
        Self {
            parameters: signature.parameters.clone().into(),
            outputs: signature.outputs.clone().into(),
            parameter_indices: signature
                .parameters
                .iter()
                .enumerate()
                .map(|(i, p)| (p.id, i))
                .collect(),
            output_indices: signature
                .outputs
                .iter()
                .enumerate()
                .map(|(i, o)| (o.id, i))
                .collect(),
        }
    }
    pub fn parameters(&self) -> &[MkMacroParameter] {
        &self.parameters
    }
    pub fn outputs(&self) -> &[MkMacroOutput] {
        &self.outputs
    }
    pub fn parameter(&self, id: MkSignatureId) -> Option<&MkMacroParameter> {
        self.parameter_indices
            .get(&id)
            .map(|i| &self.parameters[*i])
    }
    pub fn output(&self, id: MkSignatureId) -> Option<&MkMacroOutput> {
        self.output_indices.get(&id).map(|i| &self.outputs[*i])
    }
}

/// Immutable document snapshot of the selected root's dependency closure.
/// Plans retain separate step identities, playback, breakpoints and Call rows.
#[derive(Debug, Clone)]
pub struct MkCompiledProgram {
    pub root_macro_id: u64,
    order: Arc<[u64]>,
    plans: HashMap<u64, Arc<MkExecutionPlan>>,
}
impl MkCompiledProgram {
    pub fn macro_ids(&self) -> &[u64] {
        &self.order
    }
    pub fn plan(&self, id: u64) -> Option<&Arc<MkExecutionPlan>> {
        self.plans.get(&id)
    }
    pub(crate) fn root_plan_mut(&mut self) -> Option<&mut MkExecutionPlan> {
        self.plans.get_mut(&self.root_macro_id).map(Arc::make_mut)
    }
    pub fn signature(&self, id: u64) -> Option<&MkCompiledSignature> {
        self.plans.get(&id).map(|plan| &plan.signature)
    }
    pub fn name(&self, id: u64) -> Option<&str> {
        self.plans.get(&id).map(|plan| plan.name.as_str())
    }
}

/// Builds the immutable, semantically validated closure for one root run.
/// Every callee retains its full plan and its own signature and playback.
pub fn compile_program(
    document: &MkMacroDocument,
    root_macro_id: u64,
) -> Result<MkCompiledProgram, Vec<MkDiagnostic>> {
    let analysis = analyze_document(document);
    let mut diagnostics: Vec<_> = analysis
        .graph
        .root_diagnostics(root_macro_id, &analysis.diagnostics)
        .into_iter()
        .cloned()
        .collect();
    match analysis
        .graph
        .macro_index(root_macro_id)
        .map(|index| &document.macros[index])
    {
        None => diagnostics.push(MkDiagnostic::fatal(
            root_macro_id,
            None,
            "missing_root_macro",
            "Selected root macro does not exist",
        )),
        Some(root) if !root.enabled => diagnostics.push(MkDiagnostic::fatal(
            root_macro_id,
            None,
            "disabled_root_macro",
            "Selected root macro is disabled",
        )),
        _ => {}
    }
    if !can_run(&diagnostics) {
        return Err(diagnostics);
    }
    let order = analysis.graph.closure(
        root_macro_id,
        super::call_graph::DependencyPolicy::EnabledCalls,
    );
    let mut plans = HashMap::new();
    for id in &order {
        let owner = &document.macros[analysis
            .graph
            .macro_index(*id)
            .expect("validated dependency identity")];
        plans.insert(*id, Arc::new(lower_validated_macro(owner)));
    }
    Ok(MkCompiledProgram {
        root_macro_id,
        order: order.into(),
        plans,
    })
}

pub fn compile(m: &MkMacro) -> Result<MkExecutionPlan, Vec<MkDiagnostic>> {
    let doc = MkMacroDocument {
        settings: Default::default(),
        schema_version: SCHEMA_VERSION,
        macros: vec![m.clone()],
        folders: vec![],
    };
    let d = validate_document(&doc, None);
    if !can_run(&d) {
        return Err(d);
    }
    Ok(lower_validated_macro(m))
}

/// The caller has validated structure in its real document context. Lowering
/// never calls validation and never fabricates a singleton for a callee.
fn lower_validated_macro(m: &MkMacro) -> MkExecutionPlan {
    let mut ins = vec![];
    let mut map = HashMap::new();
    let mut stack: Vec<(usize, &str, Option<usize>)> = vec![];
    for s in &m.steps {
        let i = ins.len();
        map.insert(s.id, i);
        let closing = matches!(
            s.action.block_marker(),
            Some(MkBlockMarker::Else | MkBlockMarker::Close(_))
        );
        let depth = stack.len().saturating_sub(closing as usize);
        ins.push(MkInstruction {
            step: Arc::new(s.clone()),
            depth,
            jump: Jump::Next,
        });
        match s.action {
            MkAction::If(_) => stack.push((i, "if", None)),
            MkAction::Else => {
                let (_, _, e) = stack.last_mut().unwrap();
                *e = Some(i)
            }
            MkAction::EndIf => {
                let (start, _, els) = stack.pop().unwrap();
                if let Some(e) = els {
                    ins[start].jump = Jump::IfFalse(e + 1);
                    ins[e].jump = Jump::To(i + 1)
                } else {
                    ins[start].jump = Jump::IfFalse(i + 1)
                }
            }
            MkAction::RepeatStart { .. } => stack.push((i, "repeat", None)),
            MkAction::RepeatEnd => {
                let (start, _, _) = stack.pop().unwrap();
                ins[start].jump = Jump::RepeatBegin { exit: i + 1 };
                ins[i].jump = Jump::RepeatEnd {
                    start: start + 1,
                    exit: i + 1,
                };
                patch_loop(&mut ins, start, i, i, i + 1)
            }
            MkAction::WhileStart { .. } => stack.push((i, "while", None)),
            MkAction::WhileEnd => {
                let (start, _, _) = stack.pop().unwrap();
                ins[start].jump = Jump::IfFalse(i + 1);
                ins[i].jump = Jump::WhileEnd { condition: start };
                patch_loop(&mut ins, start, i, start, i + 1)
            }
            _ => {}
        }
    }
    MkExecutionPlan {
        macro_id: m.id,
        name: m.name.clone(),
        enabled: m.enabled,
        signature: MkCompiledSignature::new(&m.signature),
        playback: m.playback.clone(),
        instructions: ins.into(),
        step_to_instruction: map,
    }
}
fn patch_loop(v: &mut [MkInstruction], start: usize, end: usize, cont: usize, exit: usize) {
    for x in &mut v[start + 1..end] {
        match x.step.action {
            MkAction::Break if matches!(x.jump, Jump::Next) => x.jump = Jump::Break(exit),
            MkAction::Continue if matches!(x.jump, Jump::Next) => x.jump = Jump::Continue(cont),
            _ => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkValue;

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
            id: 1,
            name: "test".into(),
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
    fn program_keeps_transitive_plans_and_immutable_callee_metadata() {
        let mut root = mac(vec![step(
            1,
            MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 2,
                ..Default::default()
            }),
        )]);
        root.name = "root".into();
        let mut child = mac(vec![step(
            1,
            MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 3,
                ..Default::default()
            }),
        )]);
        child.id = 2;
        child.name = "child".into();
        child.playback.speed_percent = 175;
        child.steps[0].breakpoint = true;
        let mut leaf = mac(vec![step(1, MkAction::Return(Default::default()))]);
        leaf.id = 3;
        let mut unrelated = mac(vec![step(0, MkAction::Else)]);
        unrelated.id = 4;
        let mut doc = MkMacroDocument {
            macros: vec![root, child, leaf, unrelated],
            ..Default::default()
        };
        let program = compile_program(&doc, 1).unwrap();
        assert_eq!(program.macro_ids(), [1, 2, 3]);
        assert!(program.plan(4).is_none());
        assert!(matches!(
            program.plan(1).unwrap().instructions[0].step.action,
            MkAction::CallMacro(_)
        ));
        assert_eq!(program.plan(2).unwrap().playback.speed_percent, 175);
        assert!(program.plan(2).unwrap().instructions[0].step.breakpoint);
        assert_eq!(program.plan(2).unwrap().step_to_instruction[&1], 0);
        doc.macros[1].name = "edited".into();
        doc.macros[1].steps.clear();
        doc.macros[1].playback.speed_percent = 10;
        assert_eq!(program.name(2), Some("child"));
        assert_eq!(program.plan(2).unwrap().instructions.len(), 1);
        assert_eq!(program.plan(2).unwrap().playback.speed_percent, 175);
        // Singleton compilation cannot resolve a document-owned callee.
        assert!(
            compile(&doc.macros[0])
                .unwrap_err()
                .iter()
                .any(|d| d.code == "missing_call_target")
        );
    }

    #[test]
    fn program_admission_scopes_failures_and_preserves_global_identity_errors() {
        let mut unrelated = mac(vec![step(1, MkAction::Else)]);
        unrelated.id = 2;
        let mut doc = MkMacroDocument {
            macros: vec![mac(vec![]), unrelated],
            ..Default::default()
        };
        assert_eq!(compile_program(&doc, 1).unwrap().macro_ids(), [1]);
        doc.macros[0].steps.push(step(
            1,
            MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 2,
                ..Default::default()
            }),
        ));
        assert!(
            compile_program(&doc, 1)
                .unwrap_err()
                .iter()
                .any(|d| d.macro_id == 2 && d.code == "invalid_else")
        );
        doc.macros[0].steps.clear();
        for ambiguous in [0, 1] {
            doc.macros[1].id = ambiguous;
            assert!(
                compile_program(&doc, 1)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.scope == DiagnosticScope::Document && d.code == "invalid_macro_id")
            );
        }
        assert!(
            compile_program(&MkMacroDocument::default(), 90)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "missing_root_macro" && d.macro_id == 90)
        );
    }
    #[test]
    fn folder_metadata_does_not_change_compilation() {
        let mut document = MkMacroDocument {
            macros: vec![mac(vec![
                step(11, MkAction::RepeatStart { count: 2 }),
                step(
                    12,
                    MkAction::Text(MkTextPayload {
                        text: "folder-independent".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
                step(13, MkAction::RepeatEnd),
            ])],
            folders: vec![
                MkMacroFolder {
                    id: 42,
                    name: "Utilities".into(),
                },
                MkMacroFolder {
                    id: 43,
                    name: "Work".into(),
                },
            ],
            ..Default::default()
        };
        let expected = compile(&document.macros[0]).unwrap();
        let diagnostics = validate_document(&document, None);
        assert!(can_run(&diagnostics));
        for (folder_id, name) in [
            (None, "Utilities"),
            (Some(42), "Utilities"),
            (Some(42), "Renamed folder"),
            (Some(43), "Utilities"),
        ] {
            document.macros[0].folder_id = folder_id;
            document.folders[0].name = name.into();
            assert_eq!(validate_document(&document, None), diagnostics);
            // Cover every field so future plan additions must be checked here too.
            let MkExecutionPlan {
                macro_id,
                name,
                enabled,
                signature,
                playback,
                instructions,
                step_to_instruction,
            } = compile(&document.macros[0]).unwrap();
            assert_eq!(macro_id, expected.macro_id);
            assert_eq!(name, expected.name);
            assert_eq!(enabled, expected.enabled);
            assert_eq!(signature, expected.signature);
            assert_eq!(playback, expected.playback);
            assert_eq!(step_to_instruction, expected.step_to_instruction);
            assert_eq!(instructions.len(), expected.instructions.len());
            for (actual, expected) in instructions.iter().zip(expected.instructions.iter()) {
                let MkInstruction { step, depth, jump } = actual;
                assert_eq!(step, &expected.step);
                assert_eq!(depth, &expected.depth);
                assert_eq!(jump, &expected.jump);
            }
        }
    }

    #[test]
    fn if_else_jumps_and_depth() {
        let p = compile(&mac(vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(
                2,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 1,
                    ..Default::default()
                }),
            ),
            step(3, MkAction::Else),
            step(
                4,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 2,
                    ..Default::default()
                }),
            ),
            step(5, MkAction::EndIf),
        ]))
        .unwrap();
        assert_eq!(p.instructions[0].jump, Jump::IfFalse(3));
        assert_eq!(p.instructions[2].jump, Jump::To(5));
        assert_eq!(p.instructions[1].depth, 1);
        assert_eq!(p.instructions[4].depth, 0)
    }
    #[test]
    fn repeat_while_and_controls() {
        let p = compile(&mac(vec![
            step(1, MkAction::RepeatStart { count: 2 }),
            step(2, MkAction::Continue),
            step(3, MkAction::Break),
            step(4, MkAction::RepeatEnd),
            step(
                5,
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
            ),
            step(6, MkAction::WhileEnd),
        ]))
        .unwrap();
        assert_eq!(p.instructions[1].jump, Jump::Continue(3));
        assert_eq!(p.instructions[2].jump, Jump::Break(4));
        assert_eq!(p.instructions[5].jump, Jump::WhileEnd { condition: 4 })
    }
    #[test]
    fn invalid_rows_rejected() {
        assert!(compile(&mac(vec![step(1, MkAction::Else)])).is_err());
        assert!(compile(&mac(vec![step(1, MkAction::Break)])).is_err())
    }
    #[test]
    fn playback_is_carried_into_plan() {
        let mut m = mac(vec![]);
        m.playback = MkPlayback {
            speed_percent: 200,
            random_delay_ms: 7,
            random_offset_px: 9,
        };
        assert_eq!(compile(&m).unwrap().playback, m.playback);
    }

    #[test]
    fn breakpoint_metadata_is_preserved_on_the_shared_compiled_plan() {
        let mut breakpoint = step(
            2,
            MkAction::SetVariable {
                name: "selected".into(),
                value: MkValue::Boolean(true),
            },
        );
        breakpoint.breakpoint = true;
        let plan = compile(&mac(vec![
            step(
                1,
                MkAction::SetVariable {
                    name: "before".into(),
                    value: MkValue::Boolean(true),
                },
            ),
            breakpoint,
            step(
                3,
                MkAction::Text(MkTextPayload {
                    text: "after".into(),
                    mode: MkTextMode::Type,
                }),
            ),
        ]))
        .unwrap();

        assert_eq!(
            plan.instructions
                .iter()
                .map(|instruction| instruction.step.id)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            plan.instructions
                .iter()
                .map(|instruction| instruction.step.breakpoint)
                .collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(plan.step_to_instruction.get(&2), Some(&1));
    }

    #[test]
    fn compiler_marker_variants_match_editor_classification() {
        let condition = MkCondition::All { conditions: vec![] };
        let markers = [
            MkAction::If(condition.clone()),
            MkAction::Else,
            MkAction::EndIf,
            MkAction::RepeatStart { count: 1 },
            MkAction::RepeatEnd,
            MkAction::WhileStart { condition },
            MkAction::WhileEnd,
        ];
        assert!(markers.iter().all(MkAction::is_block_marker));
        assert!(!MkAction::Break.is_block_marker());
        assert!(!MkAction::Continue.is_block_marker());
    }
}
