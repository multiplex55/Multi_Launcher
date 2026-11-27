//! Canonical Calculation Pad clipboard projection.
use super::LineOutcome;

pub fn copy_all_results(
    source: &str,
    evaluation: &super::SessionEvaluation,
) -> String {
    let mut labeled = String::new();
    let mut unlabeled = String::new();
    let mut results = Vec::new();
    let analysis = super::SourceAnalysis::new(source);
    if evaluation.lines.is_empty() {
        // Preserve source even if a document-level diagnostic prevented evaluation.
        labeled.push_str(source);
        unlabeled.push_str(source);
    }
    for (index, line) in evaluation.lines.iter().enumerate() {
        let Some(expression) = source.get(line.source_range.clone()) else {
            continue;
        };
        let owner = analysis.rows.get(index).and_then(|row| row.owner).unwrap_or(index);
        let Some(owner_line) = evaluation.lines.get(owner) else { continue; };
        let outcome = match &owner_line.outcome {
            LineOutcome::Blank | LineOutcome::Comment | LineOutcome::Definition | LineOutcome::Control | LineOutcome::Continuation { .. } => None,
            LineOutcome::Value(value) => Some(
                value
                    .canonical_result()
                    .unwrap_or_else(|| "unavailable".into()),
            ),
            LineOutcome::Error(error) => Some(format!("error: {error}")),
        };
        if index == owner && outcome.is_some() {
            labeled.push_str(&format!("r{} = ", owner_line.line_index + 1));
        }
        labeled.push_str(expression);
        unlabeled.push_str(expression);
        let last = analysis.rows.get(index + 1).is_none_or(|row| row.owner != Some(owner));
        if last && let Some(outcome) = outcome {
            labeled.push_str(&format!(" = {outcome}"));
            unlabeled.push_str(&format!(" = {outcome}"));
            results.push(format!("r{} = {outcome}", owner_line.line_index + 1));
        }
        let end = evaluation
            .lines
            .get(index + 1)
            .map_or(source.len(), |next| next.source_range.start);
        if let Some(separator) = source.get(line.source_range.end..end) {
            labeled.push_str(separator);
            unlabeled.push_str(separator);
        }
    }
    if let Some(diagnostic) = &evaluation.diagnostic {
        results.push(format!("Document error: {diagnostic}"));
    }
    format!(
        "Source and results\n{labeled}\n\nResults only\n{}\n\nSource and results without row labels\n{unlabeled}",
        results.join("\n")
    )
}
