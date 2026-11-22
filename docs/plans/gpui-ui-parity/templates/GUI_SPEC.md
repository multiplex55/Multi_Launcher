# GUI specification: <stable GUI ID and name>

**Template only — unfilled.** Status: **Pending inventory**. No implementation or acceptance is claimed.

## Identity and baseline

- Goal/checkpoint:
- Parent/child surfaces and related GUI IDs:
- Baseline commit and source paths/symbols:
- Entry points: menu, command, query, shortcut, tray, child action, other:
- Owning core service/state and frontend adapter:
- Fixture/data root and known baseline limitations:

## Required parity

| Control or region ID | Current egui element/label | GPUI/GPUI Kit element | State and binding | Action/shortcut | Expected result |
|---|---|---|---|---|---|
| <fill> | <fill> | <candidate, verify API> | <fill> | <fill> | <fill> |

Record all controls, menus, context actions, tooltips, errors, progress indicators and conditional regions. GPUI Kit suitability and custom components must be explicit; a missing stock widget does not remove a required feature.

## State and interaction contract

- Initial, populated, empty, loading, error, disabled and completed states that apply:
- Selection, focus/caret, keyboard navigation and activation:
- Editing, validation, commit, cancel, Undo and dirty-state behavior:
- Close/reopen, escape, modal, dock/popout and parent/child lifecycle:
- Async completion, cancellation, stale events and disposal:
- Persistence and same-data behavior through both frontends:
- Applicable Windows integration, accessibility semantics and text input:
- Layout/resizing/scrolling, theme, typography and DPI expectations:
- Explicitly inapplicable states/properties, with reason:

## Implementation boundaries

- Existing behavior reused from shared core:
- UI-local state and lifetime:
- Ownership extracted or changed:
- GPUI Kit APIs/source revision verified:
- Custom component/adapter need and parity rationale:
- Performance-sensitive workloads and baseline observation:

## Acceptance mapping

| Requirement / interaction | Deterministic check | Native/visual check if required | Evidence ID | Current state |
|---|---|---|---|---|
| <fill> | <actual target/filter or justified manual check> | <steps + expected result> | <pending> | Pending |

## Review and closure

- Implemented source/checkpoint:
- Independent review record and findings:
- Evidence applicability at final checkpoint:
- Remaining gaps or explicit scope decision:
- Acceptance state and factual reason:

Follow [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md). Compilation or a screenshot alone is not full GUI parity.

