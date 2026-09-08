//! Session-only block visibility. All indexes are derived from current structure.
use crate::mkmacro::{StructuralBlock, StructureAnalysis};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct FoldState {
    collapsed: HashSet<(u64, u64)>,
}

pub(super) fn foldable_block(analysis: &StructureAnalysis, id: u64) -> Option<&StructuralBlock> {
    analysis.block_for_marker(id).filter(|block| {
        block.opener_id == id
            && !analysis
                .diagnostics
                .iter()
                .any(|d| block.range.contains(&d.index))
    })
}

impl FoldState {
    /// Prune on document revisions so deleted IDs cannot transfer fold choices
    /// to a later macro/step that happens to reuse the numeric identity.
    pub fn retain_document(&mut self, document: &crate::mkmacro::MkMacroDocument) {
        let openers: HashSet<_> = document
            .macros
            .iter()
            .flat_map(|m| {
                m.steps
                    .iter()
                    .filter(|step| {
                        matches!(
                            step.action.block_marker(),
                            Some(crate::mkmacro::MkBlockMarker::Open(_))
                        )
                    })
                    .map(move |step| (m.id, step.id))
            })
            .collect();
        self.collapsed.retain(|id| openers.contains(id));
    }

    pub fn is_collapsed(&self, macro_id: u64, opener: u64) -> bool {
        self.collapsed.contains(&(macro_id, opener))
    }

    pub fn toggle(&mut self, macro_id: u64, opener: u64, analysis: &StructureAnalysis) {
        if foldable_block(analysis, opener).is_some() && !self.collapsed.remove(&(macro_id, opener))
        {
            self.collapsed.insert((macro_id, opener));
        }
    }

    /// Keep source row numbers and skip the entire folded span, including Else
    /// and the closer. Child fold state is deliberately retained.
    pub fn visible_rows(&self, macro_id: u64, analysis: &StructureAnalysis) -> Vec<usize> {
        let mut rows = Vec::new();
        let mut index = 0;
        while let Some(step) = analysis.steps.get(index) {
            rows.push(index);
            index = if self.is_collapsed(macro_id, step.step_id) {
                foldable_block(analysis, step.step_id).map_or(index + 1, |b| b.closer_index + 1)
            } else {
                index + 1
            };
        }
        rows
    }

    /// A hidden row's keyboard primary is its outermost visible folded opener.
    /// The selected IDs themselves remain unchanged.
    pub fn visible_primary(&self, macro_id: u64, id: u64, analysis: &StructureAnalysis) -> u64 {
        let Some(step) = analysis.step(id) else {
            return id;
        };
        analysis
            .blocks
            .iter()
            .find(|b| {
                b.opener_index < step.index
                    && step.index <= b.closer_index
                    && self.is_collapsed(macro_id, b.opener_id)
                    && foldable_block(analysis, b.opener_id).is_some()
            })
            .map_or(id, |b| b.opener_id)
    }

    /// Shared navigation primitive: expose the requested row while leaving its
    /// own fold and unrelated/nested fold choices intact.
    pub fn expand_ancestors(&mut self, macro_id: u64, id: u64, analysis: &StructureAnalysis) {
        if let Some(step) = analysis.step(id) {
            for block in &analysis.blocks {
                if block.opener_index < step.index && step.index <= block.closer_index {
                    self.collapsed.remove(&(macro_id, block.opener_id));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkStep, analyze_structure};

    #[test]
    fn nested_folds_hide_else_and_closers_and_navigation_expands_only_ancestors() {
        let actions = [
            MkAction::If(crate::mkmacro::MkCondition::All { conditions: vec![] }),
            MkAction::RepeatStart { count: 2 },
            MkAction::Delay(Default::default()),
            MkAction::RepeatEnd,
            MkAction::Else,
            MkAction::Delay(Default::default()),
            MkAction::EndIf,
        ];
        let steps: Vec<_> = actions
            .into_iter()
            .enumerate()
            .map(|(i, action)| MkStep {
                id: i as u64 + 1,
                action,
                metadata: Default::default(),
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
            })
            .collect();
        let analysis = analyze_structure(&steps);
        let mut folds = FoldState::default();
        folds.toggle(10, 2, &analysis);
        folds.toggle(10, 1, &analysis);
        assert_eq!(folds.visible_rows(10, &analysis), [0]);
        assert_eq!(folds.visible_primary(10, 7, &analysis), 1);
        assert_eq!(folds.visible_rows(20, &analysis), [0, 1, 2, 3, 4, 5, 6]);
        folds.toggle(10, 1, &analysis);
        assert_eq!(folds.visible_rows(10, &analysis), [0, 1, 4, 5, 6]);
        folds.toggle(10, 1, &analysis);
        folds.expand_ancestors(10, 2, &analysis);
        assert!(folds.is_collapsed(10, 2));
        assert_eq!(folds.visible_rows(10, &analysis), [0, 1, 4, 5, 6]);
        folds.expand_ancestors(10, 4, &analysis);
        assert_eq!(folds.visible_rows(10, &analysis), [0, 1, 2, 3, 4, 5, 6]);
    }
}
