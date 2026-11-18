//! Pure preparation of a reference example insertion; no document is edited here.
use super::super::document::{
    rename::{RenameScope, insertion_facts},
    syntax::SourceAnalysis,
};
use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PreparedExampleInsertion {
    pub source: String,
    pub conflicting_names: Vec<String>,
    pub shifted_row_references: bool,
}

fn boundary(source: &str, analysis: &SourceAnalysis, position: usize) -> bool {
    if position == source.len() {
        return true;
    }
    if position != 0 && source.as_bytes().get(position - 1) != Some(&b'\n') {
        return false;
    }
    let Some(index) = analysis
        .rows
        .iter()
        .position(|row| row.range.start == position)
    else {
        return false;
    };
    // Blank rows in an indented block or a continued statement belong to the
    // next substantive row. Examine the original bytes, not logical row text.
    for row in &analysis.rows[index..] {
        let raw = &source[row.range.clone()];
        if row.owner.is_some() {
            return false;
        }
        let text = raw.trim_start();
        if text.is_empty() || text.starts_with('#') || text.starts_with("//") {
            continue;
        }
        if raw.starts_with(char::is_whitespace) {
            return false;
        }
        return !matches!(
            super::super::editor_control_keyword(row.text.trim()),
            Some("elif" | "else")
        );
    }
    true
}

pub(crate) fn prepare_example_insertion(
    source: &str,
    selection: Range<usize>,
    example: &str,
) -> Result<PreparedExampleInsertion, String> {
    if selection.start > selection.end {
        return Err("Invalid selection.".into());
    }
    let left = source
        .get(..selection.start)
        .ok_or("Invalid UTF-8 selection.")?;
    source
        .get(selection.clone())
        .ok_or("Invalid UTF-8 selection.")?;
    let right = source
        .get(selection.end..)
        .ok_or("Invalid UTF-8 selection.")?;
    if !selection.is_empty() && selection.start == 0 && selection.end == source.len() {
        return Err("Select less than the whole draft; Copy Example remains available.".into());
    }
    let analysis = SourceAnalysis::new(source);
    if !boundary(source, &analysis, selection.start) || !boundary(source, &analysis, selection.end)
    {
        return Err("Choose complete top-level statements or a top-level line boundary.".into());
    }
    let current = insertion_facts(source)?;
    let sample = insertion_facts(example)?;
    if selection.start != 0 && !sample.physical_rows.is_empty() {
        return Err("This example uses physical row references and can only be inserted at the start; Copy Example remains available.".into());
    }
    let retained =
        |range: &Range<usize>| range.end <= selection.start || range.start >= selection.end;
    let mut conflicts = BTreeSet::new();
    for (name, scope, declaration, _) in &sample.names {
        if !declaration || matches!(scope, RenameScope::Parameter { .. }) {
            continue;
        }
        if current
            .names
            .iter()
            .any(|(other, _, _, range)| name == other && retained(range))
            || current
                .calls
                .iter()
                .any(|(other, range)| name == other && retained(range))
        {
            conflicts.insert(name.clone());
        }
    }
    // A sample read can resolve differently when the document declares that
    // name (notably the contextual pi/pie constants). Function parameters are
    // local and are deliberately excluded.
    for (name, scope, declaration, _) in &sample.names {
        if *declaration || !matches!(scope, RenameScope::Document) {
            continue;
        }
        if current
            .names
            .iter()
            .any(|(other, existing_scope, is_declaration, range)| {
                name == other
                    && *is_declaration
                    && matches!(existing_scope, RenameScope::Document)
                    && retained(range)
            })
        {
            conflicts.insert(name.clone());
        }
    }
    for (name, _) in &sample.calls {
        if current
            .names
            .iter()
            .any(|(other, scope, declaration, range)| {
                name == other
                    && *declaration
                    && matches!(scope, RenameScope::Function { .. })
                    && range.end <= selection.start
                    && retained(range)
            })
        {
            conflicts.insert(name.clone());
        }
    }
    let eol = match source.find('\n') {
        Some(index) if index > 0 && source.as_bytes()[index - 1] == b'\r' => "\r\n",
        _ => "\n",
    };
    let normalized = example.replace("\r\n", "\n").replace('\n', eol);
    let mut insert = String::new();
    if !left.is_empty() && !left.ends_with('\n') {
        insert.push_str(eol);
    }
    insert.push_str(&normalized);
    if !right.is_empty()
        && !insert.ends_with('\n')
        && !right.starts_with('\n')
        && !right.starts_with("\r\n")
    {
        insert.push_str(eol);
    }
    let mut candidate = String::with_capacity(left.len() + insert.len() + right.len());
    candidate.push_str(left);
    candidate.push_str(&insert);
    candidate.push_str(right);
    if !super::super::limits::source_admitted(&candidate) {
        return Err("The resulting document exceeds its size limit.".into());
    }
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(index, _)| index + 1))
        .collect();
    let changed_rows = current.physical_rows.iter().any(|(number, range)| {
        if !retained(range) || *number == 0 {
            return false;
        }
        let Some(&target) = starts.get(number - 1) else {
            return candidate.bytes().filter(|b| *b == b'\n').count() + 1 >= *number;
        };
        if target >= selection.start && target < selection.end {
            return true;
        }
        if target == selection.start
            && selection.is_empty()
            && (right.starts_with('\n') || right.starts_with("\r\n"))
            && !normalized.is_empty()
        {
            return true;
        }
        let new_target = if target >= selection.end {
            target - (selection.end - selection.start) + insert.len()
        } else {
            target
        };
        candidate[..new_target]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1
            != *number
    });
    Ok(PreparedExampleInsertion {
        source: candidate,
        conflicting_names: conflicts.into_iter().collect(),
        shifted_row_references: changed_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insertion_geometry_and_collisions() {
        let source = "α = 1\r\nβ = 2\r\nα + β";
        let position = source.find("β =").unwrap();
        let result = prepare_example_insertion(source, position..position, "α = 3\nα + 1").unwrap();
        assert_eq!(result.source, "α = 1\r\nα = 3\r\nα + 1\r\nβ = 2\r\nα + β");
        assert_eq!(result.conflicting_names, ["α"]);
        assert!(prepare_example_insertion(source, 0..source.len(), "1").is_err());
        assert!(prepare_example_insertion(source, 1..1, "1").is_err());
        assert!(prepare_example_insertion("x = (\n  1 +\n  2)\n", 6..6, "1").is_err());
        assert!(prepare_example_insertion("for x in range(2):\n  x\n", 19..19, "1").is_err());
        let block = "for x in range(2):\n# comment\n  x\n";
        let comment = block.find('#').unwrap();
        assert!(prepare_example_insertion(block, comment..comment, "1").is_err());
        let source = "a = 1\r\nb = 2\r\nc = 3";
        let start = source.find("b =").unwrap();
        let end = source.find("c =").unwrap();
        assert_eq!(
            prepare_example_insertion(source, start..end, "7")
                .unwrap()
                .source,
            "a = 1\r\n7\r\nc = 3"
        );
        let mixed = "1\n2\r\n3";
        assert_eq!(
            prepare_example_insertion(mixed, 2..2, "4\n5")
                .unwrap()
                .source,
            "1\n4\n5\n2\r\n3"
        );
        let multiline = "x = (\n  1 + 2)";
        assert_eq!(
            prepare_example_insertion(multiline, multiline.len()..multiline.len(), "3")
                .unwrap()
                .source,
            "x = (\n  1 + 2)\n3"
        );
    }
    #[test]
    fn insertion_detects_physical_rows_and_local_formula_calls() {
        let source = "fn r1(x) = x\nx = 1\nr2\nr1(2)";
        let result = prepare_example_insertion(source, 0..0, "z = 3").unwrap();
        assert!(result.shifted_row_references);
        let result = prepare_example_insertion(source, source.len()..source.len(), "3").unwrap();
        assert!(!result.shifted_row_references);
        let shadow = "fn sort(x) = x";
        let result =
            prepare_example_insertion(shadow, shadow.len()..shadow.len(), "sort([1])").unwrap();
        assert_eq!(result.conflicting_names, ["sort"]);
        let source = "pi = 3";
        let result =
            prepare_example_insertion(source, source.len()..source.len(), "pi + pie - PI").unwrap();
        assert_eq!(result.conflicting_names, ["pi"]);
        assert!(prepare_example_insertion(source, source.len()..source.len(), "r1 + 1").is_err());
        assert!(
            prepare_example_insertion("fn r1(x) = x\nr1(2)", 0..0, "3")
                .unwrap()
                .shifted_row_references
                == false
        );
        assert!(
            prepare_example_insertion("r3", 0..0, "1\n2")
                .unwrap()
                .shifted_row_references
        );
        let branches = "if true:\n  1\nelse: # otherwise\n  2";
        let position = branches.find("else:").unwrap();
        assert!(prepare_example_insertion(branches, position..position, "3").is_err());
    }
}
