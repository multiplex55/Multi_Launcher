//! Pure text projection, alignment, intraline highlighting and navigation.
use regex::Regex;
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexReplacement {
    pub pattern: String,
    pub replacement: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextComparisonRules {
    pub revision: u64,
    pub line_ending_equivalence: bool,
    pub ignore_leading_whitespace: bool,
    pub ignore_trailing_whitespace: bool,
    pub ignore_all_whitespace: bool,
    pub ignore_blank_lines: bool,
    pub case_sensitive: bool,
    pub unimportant_sections: Vec<String>,
    pub replacements: Vec<RegexReplacement>,
}
impl Default for TextComparisonRules {
    fn default() -> Self {
        Self {
            revision: 0,
            line_ending_equivalence: true,
            ignore_leading_whitespace: false,
            ignore_trailing_whitespace: false,
            ignore_all_whitespace: false,
            ignore_blank_lines: false,
            case_sensitive: true,
            unimportant_sections: vec![],
            replacements: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledRules {
    revision: u64,
    rules: TextComparisonRules,
    unimportant: Vec<Regex>,
    replacements: Vec<(Regex, String)>,
}
impl CompiledRules {
    pub fn compile(rules: &TextComparisonRules) -> Result<Self, Vec<String>> {
        let mut errors = vec![];
        let unimportant = rules
            .unimportant_sections
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match Regex::new(p) {
                Ok(r) => Some(r),
                Err(e) => {
                    errors.push(format!("unimportant expression {} (`{}`): {}", i + 1, p, e));
                    None
                }
            })
            .collect();
        let replacements = rules
            .replacements
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match Regex::new(&p.pattern) {
                Ok(r) => Some((r, p.replacement.clone())),
                Err(e) => {
                    errors.push(format!(
                        "replacement expression {} (`{}`): {}",
                        i + 1,
                        p.pattern,
                        e
                    ));
                    None
                }
            })
            .collect();
        if errors.is_empty() {
            Ok(Self {
                revision: rules.revision,
                rules: rules.clone(),
                unimportant,
                replacements,
            })
        } else {
            Err(errors)
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedLine {
    pub source_line: usize,
    pub raw: String,
    pub key: String,
    pub significance_key: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProjection {
    pub lines: Vec<ProjectedLine>,
}

/// Rule order is deterministic: line endings, enabled trim/whitespace options,
/// case folding, then replacements in user order. Earlier overlapping
/// replacements therefore consume text before later rules. Unimportant regexes
/// are applied last only to `significance_key`.
pub fn project(text: &str, c: &CompiledRules) -> TextProjection {
    let mut out = vec![];
    for (source_line, raw) in split_lines(text, c.rules.line_ending_equivalence)
        .into_iter()
        .enumerate()
    {
        let mut key = raw.clone();
        if c.rules.ignore_leading_whitespace {
            key = key.trim_start().into()
        }
        if c.rules.ignore_trailing_whitespace {
            key = key.trim_end().into()
        }
        if c.rules.ignore_all_whitespace {
            key = key.chars().filter(|x| !x.is_whitespace()).collect()
        }
        if !c.rules.case_sensitive {
            key = key.to_lowercase()
        }
        for (re, repl) in &c.replacements {
            key = re.replace_all(&key, repl.as_str()).into_owned()
        }
        if c.rules.ignore_blank_lines && key.trim().is_empty() {
            continue;
        }
        let mut significance_key = key.clone();
        for re in &c.unimportant {
            significance_key = re.replace_all(&significance_key, "").into_owned()
        }
        out.push(ProjectedLine {
            source_line,
            raw,
            key,
            significance_key,
        });
    }
    TextProjection { lines: out }
}
fn split_lines(text: &str, equivalent: bool) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    if equivalent {
        text.lines().map(str::to_owned).collect()
    } else {
        text.split_inclusive('\n').map(str::to_owned).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffRowKind {
    Equal,
    Modified,
    Deleted,
    Inserted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeImportance {
    Equal,
    Unimportant,
    Important,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntralineRange {
    pub grapheme_start: usize,
    pub grapheme_end: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedDiffRow {
    pub id: u64,
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub kind: DiffRowKind,
    pub importance: ChangeImportance,
    pub left_ranges: Vec<IntralineRange>,
    pub right_ranges: Vec<IntralineRange>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub id: u64,
    pub start_row: usize,
    pub end_row: usize,
    pub important: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationIndex {
    pub all_difference_rows: Vec<usize>,
    pub important_difference_rows: Vec<usize>,
    pub hunk_boundaries: Vec<usize>,
    pub row_to_hunk: Vec<Option<usize>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextComparisonResult {
    pub left_revision: u64,
    pub right_revision: u64,
    pub rules_revision: u64,
    pub rows: Vec<AlignedDiffRow>,
    pub hunks: Vec<DiffHunk>,
    pub navigation: NavigationIndex,
    pub raw_equal: bool,
    pub equal_under_rules: bool,
}
impl TextComparisonResult {
    pub fn is_stale(&self, l: u64, r: u64, rules: u64) -> bool {
        self.left_revision != l || self.right_revision != r || self.rules_revision != rules
    }
    pub fn difference_number(&self, row: usize, important_only: bool) -> Option<(usize, usize)> {
        let v = if important_only {
            &self.navigation.important_difference_rows
        } else {
            &self.navigation.all_difference_rows
        };
        v.binary_search(&row).ok().map(|i| (i + 1, v.len()))
    }
    pub fn navigate(
        &self,
        current: Option<usize>,
        direction: NavigationDirection,
        important_only: bool,
        wrap: bool,
    ) -> Option<usize> {
        let v = if important_only {
            &self.navigation.important_difference_rows
        } else {
            &self.navigation.all_difference_rows
        };
        if v.is_empty() {
            return None;
        }
        match direction {
            NavigationDirection::First => v.first().copied(),
            NavigationDirection::Last => v.last().copied(),
            NavigationDirection::Next => {
                let c = current.unwrap_or(usize::MAX);
                v.iter()
                    .copied()
                    .find(|x| *x > c)
                    .or_else(|| wrap.then(|| v[0]))
            }
            NavigationDirection::Previous => {
                let c = current.unwrap_or(usize::MAX);
                v.iter()
                    .rev()
                    .copied()
                    .find(|x| *x < c)
                    .or_else(|| wrap.then(|| *v.last().unwrap()))
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowProjectionMode {
    #[default]
    All,
    DifferencesOnly,
}

/// A presentation-only row map. Entries are logical indices into `rows`; the
/// comparison and its stable row/hunk identities remain untouched.
pub fn row_projection(
    result: &TextComparisonResult,
    mode: RowProjectionMode,
    context: usize,
) -> Vec<usize> {
    if mode == RowProjectionMode::All {
        return (0..result.rows.len()).collect();
    }
    let mut keep = vec![false; result.rows.len()];
    for h in &result.hunks {
        let start = h.start_row.saturating_sub(context);
        let end = h.end_row.saturating_add(context).min(result.rows.len());
        keep[start..end].fill(true);
    }
    keep.into_iter()
        .enumerate()
        .filter_map(|(i, yes)| yes.then_some(i))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindScope {
    Both,
    Left,
    Right,
    CurrentSide,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindMatch {
    pub row: usize,
    pub side: crate::diff::model::DiffSide,
}

pub fn find_matches(
    result: &TextComparisonResult,
    left: &str,
    right: &str,
    needle: &str,
    scope: FindScope,
    active: crate::diff::model::DiffSide,
    case_sensitive: bool,
    projection: Option<&[usize]>,
) -> Vec<FindMatch> {
    if needle.is_empty() {
        return vec![];
    }
    let wanted = |side| match scope {
        FindScope::Both => true,
        FindScope::Left => side == crate::diff::model::DiffSide::Left,
        FindScope::Right => side == crate::diff::model::DiffSide::Right,
        FindScope::CurrentSide => side == active,
    };
    let query = if case_sensitive {
        needle.to_owned()
    } else {
        needle.to_lowercase()
    };
    let allowed = |row: usize| projection.is_none_or(|p| p.binary_search(&row).is_ok());
    let lines = |text: &str, n: usize| text.lines().nth(n).unwrap_or("").to_owned();
    let mut out = vec![];
    for (row, aligned) in result.rows.iter().enumerate().filter(|(i, _)| allowed(*i)) {
        for (side, line) in [
            (crate::diff::model::DiffSide::Left, aligned.left),
            (crate::diff::model::DiffSide::Right, aligned.right),
        ] {
            if wanted(side)
                && line.is_some_and(|n| {
                    let text = lines(
                        if side == crate::diff::model::DiffSide::Left {
                            left
                        } else {
                            right
                        },
                        n,
                    );
                    if case_sensitive {
                        text.contains(&query)
                    } else {
                        text.to_lowercase().contains(&query)
                    }
                })
            {
                out.push(FindMatch { row, side });
            }
        }
    }
    out
}

pub fn navigate_match(
    matches: &[FindMatch],
    current: Option<usize>,
    forward: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    if forward {
        current
            .and_then(|c| (c + 1 < matches.len()).then_some(c + 1))
            .or(Some(0))
    } else {
        current
            .and_then(|c| c.checked_sub(1))
            .or(Some(matches.len() - 1))
    }
}

/// Convert a y-coordinate in an aggregated overview to a logical row.
pub fn overview_row(y: f32, height: f32, row_count: usize) -> Option<usize> {
    if row_count == 0 || height <= 0.0 {
        return None;
    }
    Some((((y.clamp(0.0, height) / height) * row_count as f32).floor() as usize).min(row_count - 1))
}

#[derive(Debug, Clone, Copy)]
pub enum NavigationDirection {
    Next,
    Previous,
    First,
    Last,
}

pub fn compare(
    left: &str,
    right: &str,
    left_revision: u64,
    right_revision: u64,
    c: &CompiledRules,
    intraline_limit: usize,
) -> TextComparisonResult {
    let lp = project(left, c);
    let rp = project(right, c);
    let lk: Vec<_> = lp.lines.iter().map(|x| x.key.as_str()).collect();
    let rk: Vec<_> = rp.lines.iter().map(|x| x.key.as_str()).collect();
    let ops = capture_diff_slices(Algorithm::Myers, &lk, &rk);
    let mut rows = vec![];
    for op in ops {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for z in 0..len {
                    let kind = if lp.lines[old_index + z].raw == rp.lines[new_index + z].raw {
                        DiffRowKind::Equal
                    } else {
                        DiffRowKind::Modified
                    };
                    push_row(
                        &mut rows,
                        Some(&lp.lines[old_index + z]),
                        Some(&rp.lines[new_index + z]),
                        kind,
                        intraline_limit,
                    )
                }
            }
            DiffOp::Delete {
                old_index,
                old_len,
                new_index: _,
            } => {
                for z in 0..old_len {
                    push_row(
                        &mut rows,
                        Some(&lp.lines[old_index + z]),
                        None,
                        DiffRowKind::Deleted,
                        intraline_limit,
                    )
                }
            }
            DiffOp::Insert {
                old_index: _,
                new_index,
                new_len,
            } => {
                for z in 0..new_len {
                    push_row(
                        &mut rows,
                        None,
                        Some(&rp.lines[new_index + z]),
                        DiffRowKind::Inserted,
                        intraline_limit,
                    )
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for z in 0..paired {
                    push_row(
                        &mut rows,
                        Some(&lp.lines[old_index + z]),
                        Some(&rp.lines[new_index + z]),
                        DiffRowKind::Modified,
                        intraline_limit,
                    )
                }
                for z in paired..old_len {
                    push_row(
                        &mut rows,
                        Some(&lp.lines[old_index + z]),
                        None,
                        DiffRowKind::Deleted,
                        intraline_limit,
                    )
                }
                for z in paired..new_len {
                    push_row(
                        &mut rows,
                        None,
                        Some(&rp.lines[new_index + z]),
                        DiffRowKind::Inserted,
                        intraline_limit,
                    )
                }
            }
        }
    }
    for (i, r) in rows.iter_mut().enumerate() {
        r.id = stable_id(&(r.left, r.right, r.kind, i));
    }
    let mut hunks = vec![];
    let mut i = 0;
    while i < rows.len() {
        if rows[i].kind == DiffRowKind::Equal {
            i += 1;
            continue;
        }
        let start = i;
        while i < rows.len() && rows[i].kind != DiffRowKind::Equal {
            i += 1
        }
        let important = rows[start..i]
            .iter()
            .any(|r| r.importance == ChangeImportance::Important);
        hunks.push(DiffHunk {
            id: stable_id(&(start, i, rows[start].left, rows[start].right)),
            start_row: start,
            end_row: i,
            important,
        });
    }
    let mut nav = NavigationIndex {
        row_to_hunk: vec![None; rows.len()],
        ..Default::default()
    };
    for (hi, h) in hunks.iter().enumerate() {
        nav.hunk_boundaries.push(h.start_row);
        for row in h.start_row..h.end_row {
            nav.row_to_hunk[row] = Some(hi);
            nav.all_difference_rows.push(row);
            if rows[row].importance == ChangeImportance::Important {
                nav.important_difference_rows.push(row)
            }
        }
    }
    TextComparisonResult {
        left_revision,
        right_revision,
        rules_revision: c.revision,
        raw_equal: left == right,
        equal_under_rules: lk == rk,
        rows,
        hunks,
        navigation: nav,
    }
}
fn push_row(
    rows: &mut Vec<AlignedDiffRow>,
    l: Option<&ProjectedLine>,
    r: Option<&ProjectedLine>,
    kind: DiffRowKind,
    limit: usize,
) {
    let importance = match (l, r) {
        (Some(a), Some(b)) if a.raw == b.raw => ChangeImportance::Equal,
        (Some(a), Some(b)) if a.significance_key == b.significance_key => {
            ChangeImportance::Unimportant
        }
        _ => ChangeImportance::Important,
    };
    let (mut lr, mut rr) = (vec![], vec![]);
    if kind == DiffRowKind::Modified {
        if let (Some(a), Some(b)) = (l, r) {
            if a.raw.len() <= limit && b.raw.len() <= limit {
                (lr, rr) = intraline(&a.raw, &b.raw)
            }
        }
    }
    rows.push(AlignedDiffRow {
        id: 0,
        left: l.map(|x| x.source_line),
        right: r.map(|x| x.source_line),
        kind,
        importance,
        left_ranges: lr,
        right_ranges: rr,
    })
}
fn intraline(a: &str, b: &str) -> (Vec<IntralineRange>, Vec<IntralineRange>) {
    let ag: Vec<&str> = a.graphemes(true).collect();
    let bg: Vec<&str> = b.graphemes(true).collect();
    let mut ar = vec![];
    let mut br = vec![];
    for op in capture_diff_slices(Algorithm::Myers, &ag, &bg) {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => ar.push(range(a, &ag, old_index, old_len)),
            DiffOp::Insert {
                new_index, new_len, ..
            } => br.push(range(b, &bg, new_index, new_len)),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                ar.push(range(a, &ag, old_index, old_len));
                br.push(range(b, &bg, new_index, new_len))
            }
        }
    }
    (ar, br)
}
fn range(s: &str, g: &[&str], start: usize, len: usize) -> IntralineRange {
    let byte_start = g[..start].iter().map(|x| x.len()).sum();
    let byte_end = byte_start + g[start..start + len].iter().map(|x| x.len()).sum::<usize>();
    debug_assert!(s.is_char_boundary(byte_start) && s.is_char_boundary(byte_end));
    IntralineRange {
        grapheme_start: start,
        grapheme_end: start + len,
        byte_start,
        byte_end,
    }
}
fn stable_id<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_does_not_mutate() {
        let s = " A \n\nB".to_string();
        let mut r = TextComparisonRules::default();
        r.ignore_blank_lines = true;
        r.ignore_leading_whitespace = true;
        r.ignore_trailing_whitespace = true;
        let c = CompiledRules::compile(&r).unwrap();
        let p = project(&s, &c);
        assert_eq!(s, " A \n\nB");
        assert_eq!(
            p.lines.iter().map(|x| x.source_line).collect::<Vec<_>>(),
            [0, 2]
        )
    }
    #[test]
    fn unicode_ranges_are_boundaries() {
        let c = CompiledRules::compile(&TextComparisonRules::default()).unwrap();
        let d = compare("a👨‍👩‍👧x", "a👨‍👩‍👧y", 1, 2, &c, 100);
        for x in d.rows[0].left_ranges.iter().chain(&d.rows[0].right_ranges) {
            assert!(
                if d.rows[0].left_ranges.contains(x) {
                    "a👨‍👩‍👧x"
                } else {
                    "a👨‍👩‍👧y"
                }
                .is_char_boundary(x.byte_start)
            )
        }
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    fn compared(left: &str, right: &str) -> TextComparisonResult {
        compare(
            left,
            right,
            0,
            0,
            &CompiledRules::compile(&Default::default()).unwrap(),
            4096,
        )
    }
    #[test]
    fn context_projection_merges_nearby_hunks_and_clamps_boundaries() {
        let c = compared("a\nb\nc\nd\ne\nf", "A\nb\nc\nD\ne\nF");
        assert_eq!(
            row_projection(&c, RowProjectionMode::DifferencesOnly, 0),
            vec![0, 3, 5]
        );
        assert_eq!(
            row_projection(&c, RowProjectionMode::DifferencesOnly, 1),
            vec![0, 1, 2, 3, 4, 5]
        );
    }
    #[test]
    fn projection_preserves_row_and_hunk_identity() {
        let c = compared("a\nb\nc", "a\nB\nc");
        let p = row_projection(&c, RowProjectionMode::DifferencesOnly, 0);
        assert_eq!(c.rows[p[0]].id, c.rows[1].id);
        assert_eq!(
            c.hunks[c.navigation.row_to_hunk[p[0]].unwrap()].id,
            c.hunks[0].id
        );
    }
    #[test]
    fn find_scopes_case_and_wrapping_are_independent() {
        let c = compared("Needle\nx", "needle\ny");
        let sensitive = find_matches(
            &c,
            "Needle\nx",
            "needle\ny",
            "Needle",
            FindScope::Both,
            crate::diff::model::DiffSide::Left,
            true,
            None,
        );
        assert_eq!(sensitive.len(), 1);
        let insensitive = find_matches(
            &c,
            "Needle\nx",
            "needle\ny",
            "needle",
            FindScope::CurrentSide,
            crate::diff::model::DiffSide::Right,
            false,
            None,
        );
        assert_eq!(insensitive.len(), 1);
        assert_eq!(navigate_match(&insensitive, None, true), Some(0));
        assert_eq!(navigate_match(&insensitive, Some(0), true), Some(0));
        assert_eq!(navigate_match(&insensitive, Some(0), false), Some(0));
        assert_eq!(
            c.navigate(None, NavigationDirection::First, false, true),
            Some(0)
        );
    }
    #[test]
    fn overview_maps_coordinates() {
        assert_eq!(overview_row(0.0, 100.0, 10), Some(0));
        assert_eq!(overview_row(99.0, 100.0, 10), Some(9));
        assert_eq!(overview_row(1.0, 0.0, 10), None);
    }
}
