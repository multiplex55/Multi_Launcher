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
    pub playback: MkPlayback,
    pub instructions: Arc<[MkInstruction]>,
    pub step_to_instruction: HashMap<u64, usize>,
}
pub fn compile(m: &MkMacro) -> Result<MkExecutionPlan, Vec<MkDiagnostic>> {
    let doc = MkMacroDocument {
        settings: Default::default(),
        schema_version: SCHEMA_VERSION,
        macros: vec![m.clone()],
    };
    let d = validate_document(&doc, None);
    if !can_run(&d) {
        return Err(d);
    }
    let mut ins = vec![];
    let mut map = HashMap::new();
    let mut stack: Vec<(usize, &str, Option<usize>)> = vec![];
    for s in &m.steps {
        let i = ins.len();
        map.insert(s.id, i);
        let closing = matches!(
            s.action,
            MkAction::Else | MkAction::EndIf | MkAction::RepeatEnd | MkAction::WhileEnd
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
    Ok(MkExecutionPlan {
        macro_id: m.id,
        playback: m.playback.clone(),
        instructions: ins.into(),
        step_to_instruction: map,
    })
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
            id: 1,
            name: "test".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: Default::default(),
            steps,
        }
    }
    #[test]
    fn if_else_jumps_and_depth() {
        let p = compile(&mac(vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(2, MkAction::Delay { milliseconds: 1 }),
            step(3, MkAction::Else),
            step(4, MkAction::Delay { milliseconds: 2 }),
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
}
