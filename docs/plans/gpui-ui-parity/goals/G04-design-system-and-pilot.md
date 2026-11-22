# G04 — Create reusable Kit components and one complete real GUI

**Status: Not started.** This is an executable work packet, not a record of implementation or verification.

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective

Establish a usable visual/interaction foundation and prove a complete GUI slice with Hash Generator. This pilot validates the shared-engine and selected-window architecture before applying it across the product. It is one real feature among many, not the final deliverable.

Dependencies: G01–G03 relevant hosting/selection/core contracts. Ownership: G04 exclusively owns Hash Generator; G10 references its file-workflow integration without counting a separate port.

## Source and GUI contract

Inspect src/gui/hash_generator_dialog.rs, src/hash_generator.rs and src/plugins/hash_generator.rs, plus current utility host/presentation, theme/font, common confirmation/menu controls and tests. Inventory actual algorithms, input/file modes, options, buttons, copy/output actions, validation, loading/error behavior and supported window presentations from source/runtime; do not infer controls from a generic hash-tool design.

## G04-A — Reusable visual/interaction decisions

Define theme/text/spacing/density tokens from the existing product and user preferences. Map standard controls to the exact selected Kit release. Establish retained input/state ownership, stable control identity, common error/progress, modal/menu/tooltip behavior and keyboard focus.

Implement only the reusable pieces needed for a complete pilot and near-term launcher/settings, not an enormous speculative component framework. Use Kit components normally; record custom adaptations. Include accessible names/states/actions and visible focus for custom controls.

**Evidence:** actual rendered production components in relevant states, supported text sizes and resize conditions. **Accept when:** common behavior is reusable and the design is recognizable at launcher density. Commit the coherent foundation before feature-specific completion.

## G04-B — Pilot domain adapter and all controls

Extract any Hash Generator logic still owned by egui into its existing authoritative owner. Connect both frontend adapters to that operation. Implement the entire GPUI surface, including ordinary/empty/invalid/loading/error/success states and every existing option.

Retain file/source identity, cancellation/stale result protections, result display/copy semantics and defaults. A stock Kit control may be extended where its behavior differs. No fake hash, synchronous UI-blocking file work, hidden egui popup or omitted advanced option.

**Evidence:** existing core tests, relevant adapter/production-view interaction tests, baseline expected result comparison and live operations on synthetic data. **Accept when:** all recorded controls operate through shared behavior and action outcomes match the baseline.

## G04-C — Window, persistence and interaction parity

Implement every supported pilot mode and child dialog. Preserve close/hide/reopen, resize/scroll, keyboard/pointer operation, relevant geometry and focus return. Confirm an independent utility works while the main/root window is hidden if supported.

Use common hosting semantics without copying egui pass APIs into core. Document exact save/apply/cancel/tool-option persistence if present; preserve any intentional non-persistence of input/results. Perform relevant cross-frontend round trips.

**Evidence:** actual Windows tool interactions and comparable screenshots with same synthetic fixture. **Accept when:** controls, data, input and host lifetime satisfy the per-GUI record, not merely appearance.

## G04-D — Review and establish the repeatable slice pattern

Review the pilot’s complete controls/state/modes and engine path. Resolve concrete findings. Confirm egui still works through any extraction. Record Kit mappings/custom extensions and narrow validation patterns other goals can reuse.

Inspect whether the pilot revealed shared ownership/focus/token decisions that invalidate G01–G03 evidence; recheck only those paths. Remove temporary demo routes or clearly distinguish developer probes from production functions.

**Accept when:** the Hash Generator GUI has a complete accepted record with applicable evidence, the component/design decisions are documented, and the shared approach is ready for launcher/feature use.

## Non-goals and continuation

Do not redesign every tool now, create per-widget tests mirroring constants, or turn the pilot into a generic rendering abstraction. No repeated full suite or release relink for each style adjustment. Continue immediately with G05 and other eligible goal packets after acceptance.
