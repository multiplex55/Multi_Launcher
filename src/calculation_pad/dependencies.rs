//! Immutable dependency graph over an already-evaluated Pad document.

use super::{
    DocumentDiagnostic, DocumentId, DocumentIncarnation, LineId, LineOutcome, SessionEvaluation,
};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct DependencyRow {
    pub id: LineId,
    /// Zero-based physical source line; the visible rN is `line_index + 1`.
    pub line_index: usize,
    pub source_range: Range<usize>,
    pub source: String,
    pub assignment_name: Option<String>,
    pub outcome: LineOutcome,
    pub dependencies: Vec<LineId>,
    pub dependents: Vec<LineId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DependencySnapshot {
    pub document_id: DocumentId,
    pub incarnation: DocumentIncarnation,
    pub revision: u64,
    pub rows: Vec<DependencyRow>,
    pub diagnostic: Option<DocumentDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyLocation {
    pub line_index: usize,
    pub source_range: Range<usize>,
}

impl DependencySnapshot {
    pub(super) fn from_evaluation(
        document_id: DocumentId,
        incarnation: DocumentIncarnation,
        source: &str,
        evaluation: &SessionEvaluation,
    ) -> Option<Self> {
        let physical = evaluation
            .lines
            .iter()
            .enumerate()
            .map(|(index, row)| (row.line_index, index))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::with_capacity(evaluation.lines.len());
        for line in &evaluation.lines {
            let mut indices = line
                .dependency_indices
                .iter()
                .map(|index| physical.get(index).copied())
                .collect::<Option<Vec<_>>>()?;
            indices.sort_unstable();
            indices.dedup();
            rows.push(DependencyRow {
                id: line.id,
                line_index: line.line_index,
                source_range: line.source_range.clone(),
                source: source.get(line.source_range.clone())?.to_owned(),
                assignment_name: line.assignment_name.clone(),
                outcome: line.outcome.clone(),
                dependencies: indices
                    .into_iter()
                    .map(|index| evaluation.lines[index].id)
                    .collect(),
                dependents: Vec::new(),
            });
        }
        let by_id = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id, index))
            .collect::<HashMap<_, _>>();
        for source_index in 0..rows.len() {
            let source_id = rows[source_index].id;
            for target in rows[source_index].dependencies.clone() {
                rows[*by_id.get(&target)?].dependents.push(source_id);
            }
        }
        Some(Self {
            document_id,
            incarnation,
            revision: evaluation.revision,
            rows,
            diagnostic: evaluation.diagnostic.clone(),
        })
    }

    /// Unique reachable upstream rows in physical document order, excluding `id`.
    pub fn upstream(&self, id: LineId) -> Option<Vec<LineId>> {
        self.reachable(id, true)
    }

    /// Unique reachable downstream rows in physical document order, excluding `id`.
    pub fn downstream(&self, id: LineId) -> Option<Vec<LineId>> {
        self.reachable(id, false)
    }

    fn reachable(&self, id: LineId, upstream: bool) -> Option<Vec<LineId>> {
        let indices = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id, index))
            .collect::<HashMap<_, _>>();
        let start = *indices.get(&id)?;
        let mut seen = vec![false; self.rows.len()];
        seen[start] = true;
        let mut pending = vec![start];
        while let Some(index) = pending.pop() {
            let row = &self.rows[index];
            for neighbour in if upstream {
                &row.dependencies
            } else {
                &row.dependents
            } {
                if let Some(&next) = indices.get(neighbour)
                    && !seen[next]
                {
                    seen[next] = true;
                    pending.push(next);
                }
            }
        }
        Some(
            self.rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| (index != start && seen[index]).then_some(row.id))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculation_pad::{Diagnostic, DiagnosticKind, SessionLineResult};

    #[test]
    fn dependency_graph_deduplicates_cycles_and_preserves_global_diagnostic() {
        let ids = [LineId(1), LineId(2), LineId(3), LineId(4)];
        let outcome = LineOutcome::Error(Diagnostic {
            kind: DiagnosticKind::CyclicDependency,
        });
        let evaluation = SessionEvaluation {
            revision: 7,
            final_bindings: Default::default(),
            lines: vec![
                SessionLineResult {
                    id: ids[0],
                    line_index: 0,
                    source_range: 0..1,
                    assignment_name: None,
                    dependency_indices: vec![1, 1, 0],
                    outcome: outcome.clone(),
                },
                SessionLineResult {
                    id: ids[1],
                    line_index: 1,
                    source_range: 2..3,
                    assignment_name: None,
                    dependency_indices: vec![0],
                    outcome: outcome.clone(),
                },
                SessionLineResult {
                    id: ids[2],
                    line_index: 2,
                    source_range: 4..5,
                    assignment_name: None,
                    dependency_indices: vec![1],
                    outcome: outcome.clone(),
                },
                SessionLineResult {
                    id: ids[3],
                    line_index: 3,
                    source_range: 6..7,
                    assignment_name: None,
                    dependency_indices: vec![],
                    outcome: LineOutcome::Blank,
                },
            ],
            diagnostic: Some(DocumentDiagnostic::TooManyLines { max_lines: 256 }),
        };
        let graph = DependencySnapshot::from_evaluation(
            DocumentId(1),
            DocumentIncarnation(DocumentId(2)),
            "a\nb\nc\nd",
            &evaluation,
        )
        .unwrap();
        assert_eq!(graph.rows[0].dependencies, vec![ids[0], ids[1]]);
        assert_eq!(graph.rows[1].dependents, vec![ids[0], ids[2]]);
        assert_eq!(graph.upstream(ids[0]), Some(vec![ids[1]]));
        assert_eq!(graph.downstream(ids[0]), Some(vec![ids[1], ids[2]]));
        assert_eq!(graph.upstream(ids[3]), Some(vec![]));
        assert_eq!(graph.rows[0].outcome, outcome);
        assert_eq!(graph.diagnostic, evaluation.diagnostic);
    }
}
