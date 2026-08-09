# Deferred initiative: three-way text comparison and merge

## Scheduling

Three-way comparison is a **later initiative**. It is not part of the critical
path for completing or stabilizing the existing two-way comparison feature.
No three-way runtime types, routing, UI, or persistence should be introduced
until the two-way completion work has shipped and the model design below has
been reviewed.

## Scope

The initiative adds a text-only, base-aware comparison and merge workflow. Its
planned integration points are:

- `src/diff/three_way.rs` for the comparison and merge model;
- `src/gui/diff_dialog/three_way_view.rs` for its dedicated view; and
- `DiffView::ThreeWayText(ThreeWayTextState)` for top-level view routing.

The existing two-way `DiffSide` remains unchanged. Three-way code must define
and use a separate side/domain type whose variants represent `Left`, `Base`,
and `Right`; it must not widen `DiffSide` or make existing two-way call sites
handle a base side.

The model may reuse shared text decoding, line parsing, and comparison-rule
logic. Folder and binary data types must not be generalized merely to fit the
three-way model.

## Model design milestone

Before UI integration begins, define a deterministic text-only model that
compares both inputs to the base, correlates their edit ranges, and classifies
each result as one of:

1. unchanged;
2. changed only on the left;
3. changed only on the right;
4. the same change on both sides;
5. independent, non-conflicting changes; or
6. conflicting changes.

The design must specify how insertions are anchored, how deletion ranges are
represented, how adjacent and overlapping edits are classified, how multiple
edits at end-of-file are ordered, and how comparison rules affect equivalence.
Raw source text must remain available so rule-equivalent input is not silently
rewritten.

## Merge and save milestone

Merge actions must be explicit and conflict-aware. The design must name the
source and destination of every operation, prevent an unresolved conflict from
being presented as an automatic merge, and specify undo/redo and recomputation
behavior. It must also define the resulting document (rather than implicitly
overwriting left, base, or right) and the allowed resolutions for each conflict.

Dirty-document tracking, external-change handling, save failures, and **Save**
semantics must be consistent with two-way text comparison: failed saves remain
dirty and visible, and no operation may discard unsaved edits without an
explicit decision.

## Routing constraints

Three-way routing accepts text files only. It must explicitly reject, with a
recoverable and actionable error:

- folders;
- binary files; and
- archives or archive-backed virtual paths.

There is no planned three-way folder comparison, binary comparison, archive
comparison, synchronization, or hex-editing mode in this initiative.

## Persistence gate

Persistence is a separate, final milestone. Do not add a persisted mode or
serialize `ThreeWayTextState` until the model is stable and a migration design
has explicitly addressed:

- schema/version changes and backward compatibility;
- how left, base, right, and the merge-result destination are identified;
- restoration when one or more inputs are missing or have changed;
- whether dirty merge results can be persisted, and with what recovery rules;
- compatibility with older builds that do not recognize three-way sessions;
  and
- rejection of folder, binary, and archive sessions during restore.

## Required automated acceptance tests

The model milestone is not complete without unit tests covering:

- unchanged inputs;
- left-only and right-only changes;
- identical changes on both sides;
- independent non-conflicting changes;
- overlapping conflicts;
- insert/delete conflicts, including insertions and deletions at end-of-file;
- rule-sensitive equivalence while preserving raw text; and
- explicit routing rejection for folders, binary files, and archives.

Add merge-operation tests for explicit conflict resolution, dirty-state
transitions, successful saves, failed saves that remain dirty, and protection
against overwriting externally changed or unsaved documents.

## Delivery order

1. Review and approve the model, merge contract, and routing error contract.
2. Implement `src/diff/three_way.rs` and its complete model test matrix.
3. Add `ThreeWayTextState`, the dedicated side/domain, and
   `DiffView::ThreeWayText` without changing `DiffSide`.
4. Implement `src/gui/diff_dialog/three_way_view.rs` and explicit merge/save
   interactions.
5. Integrate text-only routing and its rejection tests.
6. Design migrations, then add persistence and restoration tests in a separate
   change.

Steps in this delivery order must remain outside the two-way completion
critical path.
