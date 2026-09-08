//! Atomic editor operations. Step identity is local to the destination macro.
use super::{
    MkAction, MkCondition, MkCoordinateTarget, MkStep, StructureAnalysis, analyze_structure,
    model::next_unused_id,
};
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

fn rewrite_pixel_target(target: &mut MkCoordinateTarget, search_ids: &BTreeMap<u64, u64>) {
    if let MkCoordinateTarget::Pixel { search_id, .. } = target
        && let Some(fresh) = search_ids.get(search_id)
    {
        *search_id = *fresh;
    }
}

fn rewrite_condition_pixel_targets(condition: &mut MkCondition, search_ids: &BTreeMap<u64, u64>) {
    match condition {
        MkCondition::PixelResult { target, .. } => rewrite_pixel_target(target, search_ids),
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            for condition in conditions {
                rewrite_condition_pixel_targets(condition, search_ids);
            }
        }
        MkCondition::Not { condition } => rewrite_condition_pixel_targets(condition, search_ids),
        MkCondition::Variable { .. }
        | MkCondition::WindowExists { .. }
        | MkCondition::WindowActive { .. }
        | MkCondition::ImageSearch { .. }
        | MkCondition::PreviousImageResult { .. } => {}
    }
}

fn rewrite_action_pixel_targets(action: &mut MkAction, search_ids: &BTreeMap<u64, u64>) {
    match action {
        MkAction::MouseMove(payload) => rewrite_pixel_target(&mut payload.target, search_ids),
        MkAction::MouseClick(payload) => rewrite_pixel_target(&mut payload.target, search_ids),
        MkAction::MouseDrag(payload) => {
            rewrite_pixel_target(&mut payload.from, search_ids);
            rewrite_pixel_target(&mut payload.to, search_ids);
        }
        MkAction::PixelCheck { target, .. } => rewrite_pixel_target(target, search_ids),
        MkAction::WaitUntil { condition, .. }
        | MkAction::If(condition)
        | MkAction::WhileStart { condition } => {
            rewrite_condition_pixel_targets(condition, search_ids);
        }
        MkAction::CallMacro(_)
        | MkAction::Return(_)
        | MkAction::KeyDown(_)
        | MkAction::KeyUp(_)
        | MkAction::KeyPress(_)
        | MkAction::Hotkey(_)
        | MkAction::Text(_)
        | MkAction::Notify(_)
        | MkAction::PlaySound(_)
        | MkAction::ClickWithinRegion(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. }
        | MkAction::Delay(_)
        | MkAction::Process(_)
        | MkAction::LauncherCommand(_)
        | MkAction::WindowActivate(_)
        | MkAction::WindowClose(_)
        | MkAction::WindowWait(_)
        | MkAction::WindowMoveResize(_)
        | MkAction::WindowState { .. }
        | MkAction::VirtualDesktop(_)
        | MkAction::SetVariable { .. }
        | MkAction::UnsetVariable { .. }
        | MkAction::PromptInput(_)
        | MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatStart { .. }
        | MkAction::RepeatEnd
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue
        | MkAction::ImageFind(_)
        | MkAction::ImageClick(_)
        | MkAction::FindPixel(_)
        | MkAction::CaptureScreenshot(_)
        | MkAction::WaitForVisualChange(_)
        | MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => {}
    }
}

/// Canonical identity boundary for copied steps. Step IDs and Find Pixel result
/// slots are local to the destination macro. Consumers copied with their
/// producer follow its fresh slot; references to producers outside the copied
/// fragment remain unchanged. External macro/signature/image references are
/// preserved verbatim.
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

    let mut used_search_ids: HashSet<_> = destination
        .iter()
        .filter_map(|step| match &step.action {
            MkAction::FindPixel(payload) if payload.search_id != 0 => Some(payload.search_id),
            _ => None,
        })
        .collect();
    let mut next_search_id = used_search_ids
        .iter()
        .max()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .unwrap_or(1);
    let mut search_id_map = BTreeMap::new();
    for step in &mut result.steps {
        if let MkAction::FindPixel(payload) = &mut step.action {
            let old = payload.search_id;
            let fresh = next_unused_id(&used_search_ids, &mut next_search_id)
                .ok_or(MutationError::IdExhausted)?;
            used_search_ids.insert(fresh);
            payload.search_id = fresh;
            // Invalid source fragments can contain duplicate producer IDs. The
            // first producer deterministically owns ambiguous copied consumers;
            // every producer still receives a distinct valid destination ID.
            search_id_map.entry(old).or_insert(fresh);
        }
    }
    for step in &mut result.steps {
        rewrite_action_pixel_targets(&mut step.action, &search_id_map);
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
    use crate::mkmacro::{
        DiagnosticSeverity, MkAction, MkCondition, MkCoordinateTarget, MkDelayPayload, MkMacro,
        MkMacroDocument, MkMouseMovePayload, MkPixelSearchPayload, MkWaitOptions,
        validate_document,
    };

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
    fn pixel_search(step_id: u64, search_id: u64) -> MkStep {
        step(
            step_id,
            MkAction::FindPixel(MkPixelSearchPayload {
                search_id,
                color: "#FFFFFF".into(),
                tolerance: 0,
                region: Default::default(),
                wait: Default::default(),
                not_found_policy: Default::default(),
                outputs: Default::default(),
            }),
        )
    }
    fn pixel_consumer(step_id: u64, search_id: u64) -> MkStep {
        step(
            step_id,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Pixel {
                    search_id,
                    offset: Default::default(),
                },
                duration_ms: 0,
            }),
        )
    }
    fn assert_no_fatal(steps: Vec<MkStep>) {
        let document = MkMacroDocument {
            macros: vec![MkMacro {
                id: 1,
                name: "Pixel copy".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                signature: Default::default(),
                steps,
            }],
            ..Default::default()
        };
        let fatal: Vec<_> = validate_document(&document, None)
            .into_iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Fatal)
            .collect();
        assert!(fatal.is_empty(), "unexpected fatal diagnostics: {fatal:?}");
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
    fn cloning_remaps_multiple_producers_and_nested_consumers_deterministically() {
        let source = vec![
            pixel_search(1, 7),
            pixel_search(2, 9),
            step(
                3,
                MkAction::WaitUntil {
                    condition: MkCondition::All {
                        conditions: vec![
                            MkCondition::PixelResult {
                                target: MkCoordinateTarget::Pixel {
                                    search_id: 7,
                                    offset: Default::default(),
                                },
                                color: "#FFFFFF".into(),
                                tolerance: 0,
                            },
                            MkCondition::Not {
                                condition: Box::new(MkCondition::PixelResult {
                                    target: MkCoordinateTarget::Pixel {
                                        search_id: 9,
                                        offset: Default::default(),
                                    },
                                    color: "#000000".into(),
                                    tolerance: 0,
                                }),
                            },
                        ],
                    },
                    wait: MkWaitOptions::default(),
                },
            ),
        ];
        let destination = vec![pixel_search(10, u64::MAX)];
        let cloned = clone_fragment(&source, &destination).unwrap();
        let producer_ids: Vec<_> = cloned
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                MkAction::FindPixel(payload) => Some(payload.search_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            producer_ids,
            [1, 2],
            "allocation must wrap deterministically"
        );
        let MkAction::WaitUntil { condition, .. } = &cloned.steps[2].action else {
            panic!("expected copied wait condition")
        };
        let MkCondition::All { conditions } = condition else {
            panic!("expected nested copied conditions")
        };
        let MkCondition::PixelResult { target, .. } = &conditions[0] else {
            panic!("expected first pixel consumer")
        };
        assert!(matches!(
            target,
            MkCoordinateTarget::Pixel { search_id: 1, .. }
        ));
        let MkCondition::Not { condition } = &conditions[1] else {
            panic!("expected nested pixel consumer")
        };
        assert!(matches!(
            condition.as_ref(),
            MkCondition::PixelResult {
                target: MkCoordinateTarget::Pixel { search_id: 2, .. },
                ..
            }
        ));
        let mut combined = destination;
        combined.extend(cloned.steps);
        assert_no_fatal(combined);
    }

    #[test]
    fn same_macro_producer_only_duplicate_gets_fresh_slot_and_preserves_external_consumer() {
        let mut steps = vec![pixel_search(1, 42), pixel_consumer(2, 42)];
        let inserted = duplicate_selection(&mut steps, &BTreeSet::from([1])).unwrap();
        assert_eq!(inserted.len(), 1);
        let copied_search_id = match &steps[1].action {
            MkAction::FindPixel(payload) => payload.search_id,
            action => panic!("expected copied producer, got {action:?}"),
        };
        assert_ne!(copied_search_id, 42);
        assert!(matches!(
            steps[2].action,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Pixel { search_id: 42, .. },
                ..
            })
        ));
        assert_no_fatal(steps);
    }

    #[test]
    fn copied_consumer_without_its_producer_preserves_the_external_slot() {
        let mut steps = vec![pixel_search(1, 42), pixel_consumer(2, 42)];
        let fragment = copy_fragment(&steps, &BTreeSet::from([2])).unwrap();
        insert_fragment(&mut steps, &fragment, InsertionAnchor::End).unwrap();

        assert!(matches!(
            steps[2].action,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Pixel { search_id: 42, .. },
                ..
            })
        ));
        assert_no_fatal(steps);
    }

    #[test]
    fn noncontiguous_producer_and_consumer_duplicate_share_the_fresh_slot() {
        let mut steps = vec![pixel_search(1, 42), delay(2), pixel_consumer(3, 42)];
        let inserted = duplicate_selection(&mut steps, &BTreeSet::from([1, 3])).unwrap();
        assert_eq!(inserted.len(), 2);
        let copied_search_id = match &steps[3].action {
            MkAction::FindPixel(payload) => payload.search_id,
            action => panic!("expected copied producer, got {action:?}"),
        };
        assert_ne!(copied_search_id, 42);
        assert!(matches!(
            steps[4].action,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Pixel { search_id, .. },
                ..
            }) if search_id == copied_search_id
        ));
        assert_no_fatal(steps);
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

    #[test]
    fn copy_matrix_covers_every_marker_family_nested_and_noncontiguous_units() {
        let steps = vec![
            delay(1),
            step(2, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(3, MkAction::RepeatStart { count: 2 }),
            delay(4),
            step(5, MkAction::RepeatEnd),
            step(6, MkAction::Else),
            step(
                7,
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
            ),
            delay(8),
            step(9, MkAction::WhileEnd),
            step(10, MkAction::EndIf),
            delay(11),
        ];
        for marker in [2, 6, 10] {
            assert_eq!(
                copy_fragment(&steps, &BTreeSet::from([marker]))
                    .unwrap()
                    .iter()
                    .map(|step| step.id)
                    .collect::<Vec<_>>(),
                (2..=10).collect::<Vec<_>>()
            );
        }
        for (marker, expected) in [(3, 3..=5), (5, 3..=5), (7, 7..=9), (9, 7..=9)] {
            assert_eq!(
                copy_fragment(&steps, &BTreeSet::from([marker]))
                    .unwrap()
                    .iter()
                    .map(|step| step.id)
                    .collect::<Vec<_>>(),
                expected.collect::<Vec<_>>()
            );
        }
        assert_eq!(
            copy_fragment(&steps, &BTreeSet::from([1, 4, 6, 8, 11]))
                .unwrap()
                .iter()
                .map(|step| step.id)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        );
        let fragment = copy_fragment(&steps, &BTreeSet::from([3])).unwrap();
        let mut destination = vec![delay(20), delay(21)];
        let middle =
            insert_fragment(&mut destination, &fragment, InsertionAnchor::After(20)).unwrap();
        let end = insert_fragment(&mut destination, &fragment, InsertionAnchor::End).unwrap();
        assert_eq!(destination.len(), 8);
        assert_eq!(middle.inserted_ids.len(), 3);
        assert_eq!(end.inserted_ids.len(), 3);
        assert!(
            middle
                .inserted_ids
                .iter()
                .all(|id| !end.inserted_ids.contains(id))
        );
        assert!(analyze_structure(&destination).diagnostics.is_empty());
    }
}
