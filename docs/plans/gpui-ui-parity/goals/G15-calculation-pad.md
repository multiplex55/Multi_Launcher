# G15 — Calculation Pad and its complete companion tools

**Status:** In implementation. **Owner:** G15 implementer with the calculation session/evaluation owner. The ordinary retained Pad, checked document workflows and focused evidence are recorded in the [P0.11 checkpoint](../CHECKPOINTS.md#p011-retained-calculation-pad-and-checked-documents). Matched native replay and the remaining G15 controls and companions stay open.

## Objective and dependencies

Recreate Calculation Pad as a complete working environment in GPUI Kit, including its structured editor, reference/help, inputs/watches, programmer inspection, dependencies, charts, and sensitivity analysis. Preserve numeric semantics and persisted documents through the existing calculation engine. This is not a replacement calculator or an opportunity to alter the language.

Depend on G00–G04, G05 routes, and G06 settings registration. Coordinate source/document transfer with other tools only where current workflows provide it. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md). G00 must enumerate each companion as its own GUI and host-mode rows.

## Verified source entry points

Pinned baseline: `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| Responsibility | Existing source |
|---|---|
| Main pad/editor | [src/gui/calculation_pad_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/calculation_pad_dialog.rs), `src/gui/calculation_pad_editor.rs`, `calculation_pad_format.rs` |
| Reference/help | `src/gui/calculation_pad_dialog/reference_help.rs`, `src/calculation_pad/reference/`, `src/calculation_pad/examples.rs` |
| Companions | `src/gui/programmer_inspector_dialog.rs`, `calculation_dependency_dialog.rs`, [calculation_chart_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/calculation_chart_dialog.rs), `calculation_sensitivity_dialog.rs` |
| Domain/session | `src/calculation_pad/{document,session,worker,persistence,input,dependencies,inspector,programmer,sensitivity,chart}.rs` and related numeric/language modules |

Inspected GUI state uses document IDs, document incarnations, line IDs, evaluation bases, host identities, revision-bound input drafts, split previews, and companion bindings. These are meaningful application identities; GPUI entities must not replace them as authoritative keys.

## Per-GUI scope

Inventory document management and unsaved decisions, source editing, syntax presentation, completion, folding, selection/caret/scroll, line results and diagnostics, split orientation/resizing, input controls/configuration, watch editing, and all menus/shortcuts. Include the full reference browser, examples and insertion decisions, and its separate hosting behavior.

Programmer Inspector includes numeric width/signedness, bases and formatting, bit display/grouping, labels, and comparison controls. Dependencies includes its navigation/selection/status. Charts include every existing chart kind, series/style/legend, axes/ticks/bounds, references, distributions, data selection, and output action. Sensitivity includes input/output selection, settings/drafts, generation/cancel/status, result table, and supported chart/other handoffs. Preserve specialized settings through G06's shell.

## Bounded checkpoint packets

### A — Establish document and evaluation authority

**Owner:** G15 with the evaluation owner. Map documents, source revisions, line identities, pending evaluations, cached sessions, and companion commands to neutral contracts. Extract editor-triggered domain actions without changing arithmetic, rounding, units, language parsing, resource limits, or persistence formats.

**Invariants:** A result applies only to its evaluation basis and document incarnation. Renaming/reordering/deleting source cannot make a stale companion mutate another line. Rehosting preserves the same cached session and pending decision. Workers are not resubmitted by redraw alone.

**Evidence and acceptance:** Baseline fixture calculations, persisted document comparisons, and existing host-migration/session checks. **Commit boundary:** Required shared contracts and egui adaptations with narrowly selected regressions.

### B — Complete the main pad and reference workflow

**Owner:** G15 frontend implementer. Recreate all editor controls, line result presentation, splits, inputs/watches/configuration, errors, document decisions, and reference/example insertion. Use Kit editor foundations and custom GPUI behavior where needed to retain exact interactions. Port reference hosting as an actual child surface, not a static help link.

**Invariants:** Caret/selection/composition and folds persist across view reconstruction. Input edits retain validation drafts and invalidate only the correct evaluation. Example/reference insertion targets the accepted current source position and preserves existing overwrite/replace decisions.

**Evidence and acceptance:** Keyboard-driven fixtures exercise multiline edits, completion, folding, input configuration, watches, errors, saved/reopened documents, and reference insertion. **Commit boundary:** Complete main-pad/reference slices with recorded ownership; no “basic calculation works” acceptance shortcut.

### C — Complete all four specialized companions

**Owner:** G15 frontend implementer, with explicit subpacket ownership. Treat Programmer Inspector, Dependencies, Charts, and Sensitivity as C1–C4, each with its own inventory, implementation commit boundary, focused evidence, and acceptance decision. Reuse calculation data and scene primitives; custom GPUI painting must preserve interaction and accessibility.

**Invariants:** Inspector changes retain width/signedness/bit-label/comparison semantics. Dependency navigation selects the intended source line. Chart controls preserve exact series/reference/axis/distribution state. Sensitivity settings and results are bound to the same document/evaluation, and cancellation or source edits cannot publish stale results as current.

**Evidence and acceptance:** Compare companion model/output snapshots against known calculations, then exercise controls and navigation in Windows. Include empty, invalid, pending, stale, and completed states, not only populated screenshots. Each C subpacket commits independently; all four are required before packet C is accepted.

### D — Verify shared-session and window continuity

**Owner:** G15 acceptance owner. Exercise the main pad with supported concurrent companions, popout/dock transitions, hidden-root completion, changed document/incarnation, closed/reopened views, and pending unsaved decisions. Check narrow/resized/scaled layouts, all focused shortcuts, and custom chart/bit-grid interaction.

**Invariants:** Companions share authoritative source/evaluation state without sharing toolkit objects. Closing a view retires its commands while preserving the engine's existing job policy. Native focus returns to the appropriate parent/editor.

**Evidence and acceptance:** Windows interaction records, command/evaluation traces, persisted document comparisons, and row-level results. **Commit boundary:** Final calculation-family corrections and evidence; affected shared editor/host changes trigger targeted earlier revalidation.

## Focused validation

The validation survey verified `gui::calculation_pad_dialog::tests::studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision`. List that exact case before selecting it under `-p multi_launcher --lib`. Discover relevant document/session/worker, input/watch, inspector, chart, dependency, and sensitivity tests from their actual modules; there is no assumption that each GUI is a separate integration binary. Run the affected numeric/domain cases only when their boundary changes. Add GPUI tests for meaningful revision-bound actions and editor interaction, retaining egui baseline coverage. Native companion lifecycle and drawing interaction need Windows evidence; a full nextest run is not required per companion.

## Exit and continuation

The main pad, reference surface, all four companions, specialized settings, and assigned host modes have complete evidence. Numerical/domain outputs and document compatibility remain unchanged. A pending companion prevents full G15 acceptance; split its work into recorded follow-on packets instead of reducing scope. Continue G16 and the larger program, with cross-tool integration and final performance checks retained for G25–G26.
