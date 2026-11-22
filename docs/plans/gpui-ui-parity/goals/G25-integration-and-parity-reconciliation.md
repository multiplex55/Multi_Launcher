# G25 — Close cross-feature gaps and account for every GUI

**Status: Not started.** This is an executable work packet, not a record of implementation or verification.

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective

Reconcile the entire source/runtime inventory and ensure all individual ports form one complete usable GPUI application. Cross-feature verification should begin as soon as paths exist; this goal closes remaining gaps rather than postponing integration until the end.

Dependencies: relevant feature implementations and their evidence. All G00 discoveries and newly found surfaces must have owners. Pending native checks remain visible and block their full acceptance.

## G25-A — Source/route/control inventory reconciliation

Revisit actual baseline window creation, Panel/routes, commands, plugin registrations/settings, widget factories, custom/native drawing and dynamic child surfaces. Compare against the final GPUI route/handler registry and per-GUI specifications.

Enumerate typed command/host/outcome handling and default methods; reject no-op/Unsupported/missing handlers counted as success. Search for egui types/runtime initialization in GPUI execution paths and shared contracts. Linking egui is expected; constructing it to fill a GPUI gap is not.

Resolve every source candidate, orphan child, mode and optional surface. Add necessary discovered work to the owning goal and implement it. Reconcile the denominator instead of declaring the old count complete.

**Accept when:** every baseline surface/control/route has an accountable implementation and evidence status, with no ownerless deferred bucket.

## G25-B — End-to-end cross-feature workflows

Exercise representative real sequences chosen from current routes: launcher to tool and back; File Search to preview/Diff/file operations; Notes/Markdown to link/diagram/export; clipboard/snippet/paste flow; Calc companion/table/chart transitions; image capture to editor/pin/copy; macro/radial/gesture actions to launch/tool operations; dashboard widgets to their feature/settings.

Preserve source/draft identity, selection, parent/child ownership, focus and intended foreground restoration. Test cancellation/error/closed-window completion where it crosses a migrated boundary. One action must not execute twice due to both launcher and child handling.

**Evidence:** use existing per-feature proof when unchanged and add only scenarios that cross a still-unproven boundary. **Accept when:** integrated user workflows match baseline outcomes and neither frontend’s core behavior regresses.

## G25-C — Persistence and lifecycle convergence

Complete affected egui→GPUI→egui round trips for canonical settings, documents, history/bookmarks/actions, plugins and relevant workspace/tool state. Preserve unsupported/unknown settings and safe recovery. Distinguish framework memory from semantic state.

Exercise repeated open/close, dock/pop, hidden root, pending jobs, unsaved edits and controlled frontend switch with complex features. Confirm service/hook/watcher ownership, cancellation, subscription cleanup and no silent side-effect replay. Reuse independent native backends safely without egui presentation leakage.

**Accept when:** both frontends remain compatible with data written by the other and critical lifetime/cleanup behavior has relevant evidence.

## G25-D — Whole-settings and acceptance freshness review

Reconcile G06’s host against every specialized plugin and widget-settings owner. Confirm every registered field/child works; a registered blank page is not parity. Resolve gaps found by the actual property editor callbacks, not only the main dialog inventory.

Audit changes since each accepted GUI’s evidence. Identify affected criteria, rerun targeted scenarios and document why retained evidence applies. Check known issues/review findings for migration-caused regressions. Do not require all unchanged tests to run again just because the program is large.

**Audit complete when:** every original criterion has a reconciled, evidence-backed status and no stale acceptance, omitted nested UI or unperformed native check is hidden in totals. **Accept this checkpoint and G25 only when their mandatory original criteria are accepted.** Unresolved criteria stay pending or blocked; an accurate audit of unfinished work is not completed parity.

## Verification budget and exit

Use focused unions by shared command, host, input, persistence or cross-feature boundary. State the concrete reason before a broader suite. No automatic full-nextest campaign or blanket pixel comparison.

Update inventory, parity, progress, review/validation and blocker records. The reviewer examines cumulative implementation for real gaps, duplicate behavior and lifecycle/data risks. When all necessary integration criteria pass, continue G26 production delivery.
