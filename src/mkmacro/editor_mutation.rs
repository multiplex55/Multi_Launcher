//! Atomic editor operations. Step identity is local to the destination macro.
use super::{MkStep, StructureAnalysis, analyze_structure, model::next_unused_id};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationError {
    MissingStep(u64),
    DuplicateId(u64),
    MalformedMarker(u64),
    InvalidStructure,
    DestinationInSelection,
    IdExhausted,
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingStep(id) => write!(f, "Step {id} no longer exists"),
            Self::DuplicateId(id) => write!(f, "Step ID {id} is ambiguous"),
            Self::MalformedMarker(id) => {
                write!(f, "Step {id} is not part of a complete, well-formed block")
            }
            Self::InvalidStructure => {
                f.write_str("The move would invalidate block nesting or change a block's parent")
            }
            Self::DestinationInSelection => {
                f.write_str("The destination is inside the selected fragment")
            }
            Self::IdExhausted => f.write_str("No unused step ID is available"),
        }
    }
}
impl std::error::Error for MutationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionUnit {
    pub first_id: u64,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedSelection {
    pub units: Vec<SelectionUnit>,
    /// Source order, independent of numeric ID ordering.
    pub ids: Vec<u64>,
}

fn unique_ids(steps: &[MkStep]) -> Result<(), MutationError> {
    let mut used = HashSet::new();
    for step in steps {
        if !used.insert(step.id) {
            return Err(MutationError::DuplicateId(step.id));
        }
    }
    Ok(())
}

pub fn normalize_selection(
    steps: &[MkStep],
    selected: &BTreeSet<u64>,
) -> Result<NormalizedSelection, MutationError> {
    unique_ids(steps)?;
    let analysis = analyze_structure(steps);
    let mut ranges = Vec::new();
    for id in selected {
        let index = analysis
            .step(*id)
            .ok_or(MutationError::MissingStep(*id))?
            .index;
        let range = if steps[index].action.is_block_marker() {
            let block = analysis
                .block_for_marker(*id)
                .ok_or(MutationError::MalformedMarker(*id))?;
            if analysis
                .diagnostics
                .iter()
                .any(|d| block.range.contains(&d.index))
            {
                return Err(MutationError::MalformedMarker(*id));
            }
            block.opener_index..block.closer_index + 1
        } else {
            index..index + 1
        };
        ranges.push(range);
    }
    ranges.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut result = NormalizedSelection::default();
    for range in ranges {
        if result
            .units
            .last()
            .is_some_and(|u| range.start < u.range.end)
        {
            continue;
        }
        result.ids.extend(steps[range.clone()].iter().map(|s| s.id));
        result.units.push(SelectionUnit {
            first_id: steps[range.start].id,
            range,
        });
    }
    Ok(result)
}

pub fn copy_fragment(
    steps: &[MkStep],
    selected: &BTreeSet<u64>,
) -> Result<Vec<MkStep>, MutationError> {
    let normalized = normalize_selection(steps, selected)?;
    Ok(normalized
        .units
        .iter()
        .flat_map(|u| steps[u.range.clone()].iter().cloned())
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClonedFragment {
    pub steps: Vec<MkStep>,
    pub id_map: BTreeMap<u64, u64>,
    pub inserted_ids: Vec<u64>,
}

/// Canonical identity boundary for copied steps. Pixel search IDs name result
/// variables (including references embedded in variable strings), not steps.
/// Preserve these and external macro/signature/image references verbatim.
/// There are currently no action payload references to step IDs.
pub fn clone_fragment(
    fragment: &[MkStep],
    destination: &[MkStep],
) -> Result<ClonedFragment, MutationError> {
    unique_ids(fragment)?;
    unique_ids(destination)?;
    let mut used: HashSet<_> = destination.iter().chain(fragment).map(|s| s.id).collect();
    let mut next = used
        .iter()
        .max()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .unwrap_or(1);
    let mut result = ClonedFragment {
        steps: fragment.to_vec(),
        id_map: BTreeMap::new(),
        inserted_ids: Vec::new(),
    };
    for step in &mut result.steps {
        let fresh = next_unused_id(&used, &mut next).ok_or(MutationError::IdExhausted)?;
        used.insert(fresh);
        result.id_map.insert(step.id, fresh);
        step.id = fresh;
        result.inserted_ids.push(fresh);
    }
    Ok(result)
}

/// A boundary resolved against current stable IDs at application time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionAnchor {
    Before(u64),
    After(u64),
    End,
}

fn insertion_index(steps: &[MkStep], anchor: InsertionAnchor) -> Result<usize, MutationError> {
    let (id, after) = match anchor {
        InsertionAnchor::End => return Ok(steps.len()),
        InsertionAnchor::Before(id) => (id, false),
        InsertionAnchor::After(id) => (id, true),
    };
    steps
        .iter()
        .position(|s| s.id == id)
        .map(|i| i + usize::from(after))
        .ok_or(MutationError::MissingStep(id))
}

// Ignore row indexes, which naturally change on a move. Compare the identities
// of malformed markers and every complete marker relationship instead.
fn same_structure(before: &StructureAnalysis, after: &StructureAnalysis) -> bool {
    before.diagnostics.len() == after.diagnostics.len()
        && before.diagnostics.iter().all(|d| {
            after
                .diagnostics
                .iter()
                .any(|a| a.step_id == d.step_id && a.kind == d.kind)
        })
        && before.blocks.len() == after.blocks.len()
        && before.blocks.iter().all(|b| {
            after
                .blocks
                .iter()
                .find(|a| a.opener_id == b.opener_id)
                .is_some_and(|a| {
                    a.opener_id == b.opener_id
                        && a.kind == b.kind
                        && a.closer_id == b.closer_id
                        && a.else_marker.map(|x| x.0) == b.else_marker.map(|x| x.0)
                        && before.step(b.opener_id).and_then(|s| s.containing_block)
                            == after.step(a.opener_id).and_then(|s| s.containing_block)
                })
        })
}

pub fn insert_fragment(
    steps: &mut Vec<MkStep>,
    fragment: &[MkStep],
    anchor: InsertionAnchor,
) -> Result<ClonedFragment, MutationError> {
    // A fragment must contain every marker's complete block. Ordinary control
    // actions are retained; semantic validation reports their destination scope.
    let all = fragment.iter().map(|s| s.id).collect();
    normalize_selection(fragment, &all)?;
    let index = insertion_index(steps, anchor)?;
    let cloned = clone_fragment(fragment, steps)?;
    let mut candidate = steps.clone();
    candidate.splice(index..index, cloned.steps.iter().cloned());
    let before = analyze_structure(steps);
    let after = analyze_structure(&candidate);
    // New blocks are allowed, but inserting complete units may not repair or
    // steal existing malformed/complete marker relationships.
    let inserted: HashSet<_> = cloned.inserted_ids.iter().copied().collect();
    let mut existing_after = after.clone();
    existing_after
        .blocks
        .retain(|b| !inserted.contains(&b.opener_id));
    if !same_structure(&before, &existing_after) {
        return Err(MutationError::InvalidStructure);
    }
    *steps = candidate;
    Ok(cloned)
}

pub fn duplicate_selection(
    steps: &mut Vec<MkStep>,
    selected: &BTreeSet<u64>,
) -> Result<BTreeSet<u64>, MutationError> {
    let fragment = copy_fragment(steps, selected)?;
    let anchor = fragment
        .last()
        .map_or(InsertionAnchor::End, |s| InsertionAnchor::After(s.id));
    Ok(insert_fragment(steps, &fragment, anchor)?
        .inserted_ids
        .into_iter()
        .collect())
}

pub fn delete_selection(
    steps: &mut Vec<MkStep>,
    selected: &BTreeSet<u64>,
) -> Result<BTreeSet<u64>, MutationError> {
    let normalized = normalize_selection(steps, selected)?;
    let (Some(first), Some(last)) = (normalized.units.first(), normalized.units.last()) else {
        return Ok(BTreeSet::new());
    };
    let fallback = steps
        .get(last.range.end)
        .or_else(|| first.range.start.checked_sub(1).and_then(|i| steps.get(i)))
        .map(|s| s.id);
    let remove: HashSet<_> = normalized.ids.into_iter().collect();
    steps.retain(|s| !remove.contains(&s.id));
    Ok(fallback.into_iter().collect())
}

fn shift_rows(steps: &mut [MkStep], ids: &BTreeSet<u64>, down: bool) {
    if down {
        for i in (0..steps.len().saturating_sub(1)).rev() {
            if ids.contains(&steps[i].id) && !ids.contains(&steps[i + 1].id) {
                steps.swap(i, i + 1);
            }
        }
    } else {
        for i in 1..steps.len() {
            if ids.contains(&steps[i].id) && !ids.contains(&steps[i - 1].id) {
                steps.swap(i, i - 1);
            }
        }
    }
}

pub fn move_selection(
    steps: &mut [MkStep],
    selected: &BTreeSet<u64>,
    down: bool,
) -> Result<BTreeSet<u64>, MutationError> {
    let expanded = normalize_selection(steps, selected)?
        .ids
        .into_iter()
        .collect();
    let before = analyze_structure(steps);
    let mut candidate = steps.to_vec();
    loop {
        let previous = candidate.clone();
        shift_rows(&mut candidate, &expanded, down);
        if candidate == previous {
            if same_structure(&before, &analyze_structure(&candidate)) {
                break;
            }
            // A complete block at its parent's boundary cannot leave that
            // parent. Ordinary rows preserve the existing cross-boundary move.
            return Ok(expanded);
        }
        if same_structure(&before, &analyze_structure(&candidate)) {
            break;
        }
    }
    steps.clone_from_slice(&candidate);
    Ok(expanded)
}

pub fn move_to(
    steps: &mut Vec<MkStep>,
    selected: &BTreeSet<u64>,
    anchor: InsertionAnchor,
) -> Result<Vec<u64>, MutationError> {
    let normalized = normalize_selection(steps, selected)?;
    let ids: HashSet<_> = normalized.ids.iter().copied().collect();
    if matches!(anchor, InsertionAnchor::Before(id) | InsertionAnchor::After(id) if ids.contains(&id))
    {
        return Err(MutationError::DestinationInSelection);
    }
    let before = analyze_structure(steps);
    let mut candidate: Vec<_> = steps
        .iter()
        .filter(|s| !ids.contains(&s.id))
        .cloned()
        .collect();
    let index = insertion_index(&candidate, anchor)?;
    candidate.splice(
        index..index,
        steps.iter().filter(|s| ids.contains(&s.id)).cloned(),
    );
    if !same_structure(&before, &analyze_structure(&candidate)) {
        return Err(MutationError::InvalidStructure);
    }
    *steps = candidate;
    Ok(normalized.ids)
}

fn resolved(steps: &[MkStep], marker_id: u64) -> Result<super::StructuralBlock, MutationError> {
    normalize_selection(steps, &BTreeSet::from([marker_id]))?;
    analyze_structure(steps)
        .block_for_marker(marker_id)
        .cloned()
        .ok_or(MutationError::MalformedMarker(marker_id))
}
pub fn delete_block(
    steps: &mut Vec<MkStep>,
    marker_id: u64,
) -> Result<super::MutationResult, MutationError> {
    let b = resolved(steps, marker_id)?;
    let first = b.opener_index;
    let following_id = steps.get(b.closer_index + 1).map(|s| s.id);
    let preceding_id = first
        .checked_sub(1)
        .and_then(|i| steps.get(i))
        .map(|s| s.id);
    steps.drain(b.range);
    Ok(super::MutationResult {
        first_removed_index: first,
        following_id,
        preceding_id,
        first_preserved_body_id: None,
    })
}
pub fn unwrap_block(
    steps: &mut Vec<MkStep>,
    marker_id: u64,
) -> Result<super::MutationResult, MutationError> {
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
    Ok(super::MutationResult {
        first_removed_index: first,
        following_id,
        preceding_id,
        first_preserved_body_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkCondition, MkDelayPayload};

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            action,
            enabled: true,
            breakpoint: true,
            repeat: 2,
            delay_after_ms: 7,
            on_error: Default::default(),
            metadata: Default::default(),
        }
    }
    fn delay(id: u64) -> MkStep {
        step(id, MkAction::Delay(MkDelayPayload::default()))
    }
    fn nested() -> Vec<MkStep> {
        vec![
            delay(90),
            step(10, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(20, MkAction::RepeatStart { count: 2 }),
            delay(30),
            step(40, MkAction::RepeatEnd),
            step(50, MkAction::Else),
            delay(60),
            step(70, MkAction::EndIf),
            delay(80),
        ]
    }

    #[test]
    fn normalization_resolves_all_markers_and_deduplicates_in_source_order() {
        let steps = nested();
        for marker in [10, 50, 70] {
            let selected =
                normalize_selection(&steps, &BTreeSet::from([marker, 20, 30, 80])).unwrap();
            assert_eq!(selected.units.len(), 2);
            assert_eq!(selected.ids, [10, 20, 30, 40, 50, 60, 70, 80]);
        }
        for marker in [20, 40] {
            assert_eq!(
                normalize_selection(&steps, &BTreeSet::from([marker]))
                    .unwrap()
                    .ids,
                [20, 30, 40]
            );
        }
    }

    #[test]
    fn malformed_marker_rejects_every_mutation_atomically() {
        let mut steps = vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(2, MkAction::Else),
            step(3, MkAction::Else),
            step(4, MkAction::EndIf),
        ];
        let original = steps.clone();
        for id in [1, 2, 3, 4] {
            let selected = BTreeSet::from([id]);
            assert!(matches!(
                normalize_selection(&steps, &selected),
                Err(MutationError::MalformedMarker(_))
            ));
            assert!(duplicate_selection(&mut steps, &selected).is_err());
            assert!(delete_selection(&mut steps, &selected).is_err());
            assert!(move_selection(&mut steps, &selected, true).is_err());
            assert!(unwrap_block(&mut steps, id).is_err());
            assert_eq!(steps, original);
        }
    }

    #[test]
    fn cloning_after_overflow_preserves_every_non_identity_field() {
        let mut source = vec![delay(u64::MAX), delay(5)];
        source[0].metadata.label = "label".into();
        source[0].metadata.comment = "comment".into();
        source[0].metadata.bookmarked = true;
        source[0].metadata.accent = crate::mkmacro::MkStepAccent::Purple;
        let destination = vec![delay(1), delay(2), delay(6)];
        let cloned = clone_fragment(&source, &destination).unwrap();
        assert_eq!(cloned.inserted_ids, [3, 4]);
        for (original, copy) in source.iter().zip(&cloned.steps) {
            let mut expected = original.clone();
            expected.id = cloned.id_map[&original.id];
            assert_eq!(*copy, expected);
        }
        assert_eq!(source[0].id, u64::MAX);
    }

    #[test]
    fn cloning_preserves_pixel_producer_and_consumer_result_slot() {
        use crate::mkmacro::{MkCoordinateTarget, MkMouseMovePayload, MkPixelSearchPayload};
        let source = vec![
            step(
                42,
                MkAction::FindPixel(MkPixelSearchPayload {
                    search_id: 42,
                    color: "#FFFFFF".into(),
                    tolerance: 0,
                    region: Default::default(),
                    wait: Default::default(),
                    not_found_policy: Default::default(),
                    outputs: Default::default(),
                }),
            ),
            step(
                43,
                MkAction::MouseMove(MkMouseMovePayload {
                    target: MkCoordinateTarget::Pixel {
                        search_id: 42,
                        offset: Default::default(),
                    },
                    duration_ms: 0,
                }),
            ),
        ];
        let cloned = clone_fragment(&source, &[]).unwrap();
        assert_ne!(cloned.steps[0].id, 42);
        assert_eq!(cloned.steps[0].action, source[0].action);
        assert_eq!(cloned.steps[1].action, source[1].action);
    }

    #[test]
    fn duplication_inserts_complete_block_and_selects_all_new_rows() {
        let mut steps = nested();
        let original = steps.clone();
        let inserted = duplicate_selection(&mut steps, &BTreeSet::from([50, 30])).unwrap();
        assert_eq!(inserted.len(), 7);
        assert_eq!(steps.len(), 16);
        assert!(analyze_structure(&steps).diagnostics.is_empty());
        assert_eq!(&steps[..8], &original[..8]);
        assert_eq!(steps.last(), original.last());
        assert!(steps[8..15].iter().all(|s| inserted.contains(&s.id)));
    }

    #[test]
    fn drag_is_atomic_preserves_ids_and_rejects_changed_block_parent() {
        let mut steps = nested();
        let original = steps.clone();
        assert_eq!(
            move_to(
                &mut steps,
                &BTreeSet::from([20]),
                InsertionAnchor::Before(90)
            ),
            Err(MutationError::InvalidStructure)
        );
        assert_eq!(steps, original);
        assert_eq!(
            move_to(
                &mut steps,
                &BTreeSet::from([10]),
                InsertionAnchor::Before(30)
            ),
            Err(MutationError::DestinationInSelection)
        );
        assert_eq!(steps, original);
        let ids = move_to(&mut steps, &BTreeSet::from([10]), InsertionAnchor::End).unwrap();
        assert_eq!(ids, [10, 20, 30, 40, 50, 60, 70]);
        assert_eq!(
            steps.iter().map(|s| s.id).collect::<Vec<_>>(),
            [90, 80, 10, 20, 30, 40, 50, 60, 70]
        );
        assert!(analyze_structure(&steps).diagnostics.is_empty());
    }
}
