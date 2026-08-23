//! Tolerant, editor-oriented analysis and mutations for macro block structure.
use super::{MkAction, MkBlockKind, MkBlockMarker, MkStep};
use std::{collections::HashMap, ops::RangeInclusive};

pub type BlockKind = MkBlockKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralBlock {
    pub kind: BlockKind,
    pub opener_id: u64,
    pub opener_index: usize,
    pub else_marker: Option<(u64, usize)>,
    pub closer_id: u64,
    pub closer_index: usize,
    pub range: RangeInclusive<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepStructure {
    pub step_id: u64,
    pub index: usize,
    pub depth: usize,
    /// Stable ID of the innermost complete containing block's opener.
    pub containing_block: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureDiagnosticKind {
    UnmatchedCloser,
    ElseWithoutIf,
    DuplicateElse,
    MismatchedCloser {
        expected: BlockKind,
        found: BlockKind,
    },
    UnclosedOpener,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureDiagnostic {
    pub kind: StructureDiagnosticKind,
    pub step_id: u64,
    pub index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StructureAnalysis {
    pub blocks: Vec<StructuralBlock>,
    pub steps: Vec<StepStructure>,
    pub diagnostics: Vec<StructureDiagnostic>,
    block_by_marker_id: HashMap<u64, usize>,
    step_by_id: HashMap<u64, usize>,
}
impl StructureAnalysis {
    /// Resolves an opener, Else, or closer stable ID to its complete block.
    pub fn block_for_marker(&self, id: u64) -> Option<&StructuralBlock> {
        self.block_by_marker_id
            .get(&id)
            .and_then(|i| self.blocks.get(*i))
    }
    pub fn step(&self, id: u64) -> Option<&StepStructure> {
        self.step_by_id.get(&id).and_then(|i| self.steps.get(*i))
    }
    pub fn containing_block(&self, id: u64) -> Option<&StructuralBlock> {
        let opener = self.step(id)?.containing_block?;
        self.block_for_marker(opener)
    }
}

#[derive(Clone, Copy)]
struct Open {
    kind: BlockKind,
    id: u64,
    index: usize,
    els: Option<(u64, usize)>,
}

pub fn analyze_structure(steps: &[MkStep]) -> StructureAnalysis {
    let mut result = StructureAnalysis::default();
    let mut stack: Vec<Open> = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let marker = step.action.block_marker();
        let closing_like = matches!(marker, Some(MkBlockMarker::Else | MkBlockMarker::Close(_)));
        let depth = stack.len().saturating_sub(usize::from(closing_like));
        result.steps.push(StepStructure {
            step_id: step.id,
            index,
            depth,
            containing_block: None,
        });
        result.step_by_id.entry(step.id).or_insert(index);
        if let Some(MkBlockMarker::Open(kind)) = marker {
            stack.push(Open {
                kind,
                id: step.id,
                index,
                els: None,
            });
        } else if matches!(marker, Some(MkBlockMarker::Else)) {
            match stack.last_mut() {
                Some(top) if top.kind == BlockKind::If && top.els.is_none() => {
                    top.els = Some((step.id, index))
                }
                Some(top) if top.kind == BlockKind::If => {
                    result.diagnostics.push(StructureDiagnostic {
                        kind: StructureDiagnosticKind::DuplicateElse,
                        step_id: step.id,
                        index,
                    })
                }
                _ => result.diagnostics.push(StructureDiagnostic {
                    kind: StructureDiagnosticKind::ElseWithoutIf,
                    step_id: step.id,
                    index,
                }),
            }
        } else if let Some(MkBlockMarker::Close(kind)) = marker {
            match stack.last().copied() {
                None => result.diagnostics.push(StructureDiagnostic {
                    kind: StructureDiagnosticKind::UnmatchedCloser,
                    step_id: step.id,
                    index,
                }),
                Some(top) if top.kind != kind => result.diagnostics.push(StructureDiagnostic {
                    kind: StructureDiagnosticKind::MismatchedCloser {
                        expected: top.kind,
                        found: kind,
                    },
                    step_id: step.id,
                    index,
                }),
                Some(top) => {
                    stack.pop();
                    result.blocks.push(StructuralBlock {
                        kind,
                        opener_id: top.id,
                        opener_index: top.index,
                        else_marker: top.els,
                        closer_id: step.id,
                        closer_index: index,
                        range: top.index..=index,
                    });
                }
            }
        }
    }
    for open in stack {
        result.diagnostics.push(StructureDiagnostic {
            kind: StructureDiagnosticKind::UnclosedOpener,
            step_id: open.id,
            index: open.index,
        });
    }
    result.blocks.sort_by_key(|b| b.opener_index);
    for (bi, b) in result.blocks.iter().enumerate() {
        result.block_by_marker_id.insert(b.opener_id, bi);
        result.block_by_marker_id.insert(b.closer_id, bi);
        if let Some((id, _)) = b.else_marker {
            result.block_by_marker_id.insert(id, bi);
        }
    }
    // Resolve only complete containers; walk outward if the immediate draft opener is incomplete.
    for (i, info) in result.steps.iter_mut().enumerate() {
        info.containing_block = result
            .blocks
            .iter()
            .filter(|b| b.opener_index < i && i < b.closer_index)
            .max_by_key(|b| b.opener_index)
            .map(|b| b.opener_id);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult {
    pub first_removed_index: usize,
    pub following_id: Option<u64>,
    pub preceding_id: Option<u64>,
    pub first_preserved_body_id: Option<u64>,
}

fn resolved(steps: &[MkStep], marker_id: u64) -> Result<StructuralBlock, String> {
    analyze_structure(steps)
        .block_for_marker(marker_id)
        .cloned()
        .ok_or_else(|| format!("Step {marker_id} is not part of a complete block"))
}
pub fn delete_block(steps: &mut Vec<MkStep>, marker_id: u64) -> Result<MutationResult, String> {
    let b = resolved(steps, marker_id)?;
    let first = b.opener_index;
    let following_id = steps.get(b.closer_index + 1).map(|s| s.id);
    let preceding_id = first
        .checked_sub(1)
        .and_then(|i| steps.get(i))
        .map(|s| s.id);
    steps.drain(b.range);
    Ok(MutationResult {
        first_removed_index: first,
        following_id,
        preceding_id,
        first_preserved_body_id: None,
    })
}
pub fn unwrap_block(steps: &mut Vec<MkStep>, marker_id: u64) -> Result<MutationResult, String> {
    let b = resolved(steps, marker_id)?;
    let first = b.opener_index;
    let marker_ids = [
        Some(b.opener_id),
        b.else_marker.map(|x| x.0),
        Some(b.closer_id),
    ];
    let first_preserved_body_id = steps[b.opener_index + 1..b.closer_index]
        .iter()
        .find(|s| !marker_ids.contains(&Some(s.id)))
        .map(|s| s.id);
    let following_id = steps.get(b.closer_index + 1).map(|s| s.id);
    let preceding_id = first
        .checked_sub(1)
        .and_then(|i| steps.get(i))
        .map(|s| s.id);
    steps.retain(|s| !marker_ids.contains(&Some(s.id)));
    Ok(MutationResult {
        first_removed_index: first,
        following_id,
        preceding_id,
        first_preserved_body_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkCondition, MkErrorPolicy};
    fn s(id: u64, a: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action: a,
        }
    }
    fn delay(id: u64) -> MkStep {
        s(id, MkAction::Delay { milliseconds: 1 })
    }
    #[test]
    fn nested_relationships_and_lookup() {
        let v = vec![
            s(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            s(2, MkAction::RepeatStart { count: 2 }),
            s(
                3,
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
            ),
            delay(4),
            s(5, MkAction::WhileEnd),
            s(6, MkAction::RepeatEnd),
            s(7, MkAction::Else),
            delay(8),
            s(9, MkAction::EndIf),
        ];
        let a = analyze_structure(&v);
        assert!(a.diagnostics.is_empty());
        assert_eq!(a.blocks.len(), 3);
        for id in [1, 7, 9] {
            assert_eq!(a.block_for_marker(id).unwrap().range, 0..=8);
        }
        assert_eq!(a.containing_block(4).unwrap().kind, BlockKind::While);
        assert_eq!(a.step(4).unwrap().depth, 3);
        assert_eq!(a.block_for_marker(7).unwrap().else_marker, Some((7, 6)));
    }
    #[test]
    fn malformed_sequences_never_panic_and_diagnose() {
        let cases = vec![
            vec![s(1, MkAction::EndIf)],
            vec![s(1, MkAction::Else)],
            vec![s(1, MkAction::If(MkCondition::All { conditions: vec![] }))],
            vec![
                s(1, MkAction::If(MkCondition::All { conditions: vec![] })),
                s(2, MkAction::Else),
                s(3, MkAction::Else),
                s(4, MkAction::EndIf),
            ],
            vec![
                s(1, MkAction::RepeatStart { count: 1 }),
                s(2, MkAction::WhileEnd),
            ],
        ];
        for v in cases {
            assert!(!analyze_structure(&v).diagnostics.is_empty());
        }
    }
    #[test]
    fn controls_are_not_markers() {
        assert!(!MkAction::Break.is_block_marker());
        assert!(!MkAction::Continue.is_block_marker());
    }
    #[test]
    fn mutations_delete_and_unwrap() {
        let mut v = vec![
            delay(9),
            s(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            delay(2),
            s(3, MkAction::Else),
            delay(4),
            s(5, MkAction::EndIf),
            delay(10),
        ];
        let r = unwrap_block(&mut v, 3).unwrap();
        assert_eq!(
            v.iter().map(|x| x.id).collect::<Vec<_>>(),
            vec![9, 2, 4, 10]
        );
        assert_eq!(r.first_preserved_body_id, Some(2));
        let mut v = vec![
            s(1, MkAction::RepeatStart { count: 1 }),
            delay(2),
            s(3, MkAction::RepeatEnd),
        ];
        delete_block(&mut v, 3).unwrap();
        assert!(v.is_empty());
    }
}
