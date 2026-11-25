# G04 — Create reusable Kit components and one complete real GUI

**Status: In implementation.** The real Kit Hash Generator has shared Text, File and Clipboard operations, native picker, visual and accessible worker progress, retained Dock and Pop Out, and persisted keyboard and pointer geometry. The compact footer is reachable with the wheel and scrollbar in both hosts. The popout calculated while the launcher was hidden by the configured app-launch policy and rehosted its exact result through Dock. The matched Clipboard Load, digest, Copy, File progress and cancellation replays passed in both frontends. Scoped Windows focus and main-control and dropdown Jump evidence exists for both Hash hosts. Mixed DPI, global invocation, and the remaining component and presentation criteria are open. See the [Hash Generator record](../gui/hash-generator.md) and [checkpoints](../CHECKPOINTS.md).

Matched native replay closed each popout during live File progress and reopened a fresh Text session. The shared GPUI view now keeps progress, digest and calculation actions together in both hosts. The initial standard popout shows those actions without scrolling, and a Comfortable 150% narrow dock wraps controls and retains visible Jump targets. [Native close evidence](../VALIDATION_LOG.md#g04-native-hash-close-and-reopen-during-file-calculation) · [Layout evidence](../VALIDATION_LOG.md#g04-compact-hash-viewport-actions)

Matched Windows docks and popouts display the full SHA-512 digest. GPUI styles exact data with the resolved monospace font and exposes the raw result in one accessible label, including at Comfortable 150% in the narrow dock. [Exact-data evidence](../VALIDATION_LOG.md#g04-hash-exact-data-typography-and-digest)

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

G04-C1 moves effective preference, accepted changes and worker acknowledgements into one frontend-neutral owner with the existing egui context and repaint adapter. G04-C2 uses it for the retained Hash preview in real docked and popped-out Kit hosts. Land this boundary before the complete controls so window-bound entities and subscriptions need one mount path. The first round trip uses the canonical `hash-generator` key and the existing settings writer. The GPUI writer must settle before the G03 restart admits another owner.

**Evidence:** actual Windows tool interactions and comparable screenshots with same synthetic fixture. **Accept when:** controls, data, input and host lifetime satisfy the per-GUI record, not merely appearance.

## G04-D — Review and establish the repeatable slice pattern

Review the pilot’s complete controls/state/modes and engine path. Resolve concrete findings. Confirm egui still works through any extraction. Record Kit mappings/custom extensions and narrow validation patterns other goals can reuse.

Inspect whether the pilot revealed shared ownership/focus/token decisions that invalidate G01–G03 evidence; recheck only those paths. Remove temporary demo routes or clearly distinguish developer probes from production functions.

**Accept when:** the Hash Generator GUI has a complete accepted record with applicable evidence, the component/design decisions are documented, and the shared approach is ready for launcher/feature use.

## Non-goals and continuation

Do not redesign every tool now, create per-widget tests mirroring constants, or turn the pilot into a generic rendering abstraction. No repeated full suite or release relink for each style adjustment. Continue immediately with G05 and other eligible goal packets after acceptance.
