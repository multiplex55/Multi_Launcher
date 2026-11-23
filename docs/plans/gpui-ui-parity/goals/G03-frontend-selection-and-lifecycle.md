# G03 — Complete selection, switching and fallback semantics

**Status: In implementation.** The startup truth table, explicit duplicate feedback and native checked idle restart in both directions have evidence. The same-profile round trip preserved the saved Actions catalog. Dirty Settings, a paused Stopwatch, a queued Duplicate and an active invocation service have specific refusal receipts. Broader active-owner retirement and cross-frontend persistence remain open. See [frontend selection](../FRONTEND_SELECTION.md) and the [validation log](../VALIDATION_LOG.md).

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective

Turn G01’s foundation selector into the real product contract: explicit frontend choice, bare-launch egui, canonical data ownership, clear existing-instance behavior, controlled switching and independently usable fallback.

Dependencies: G01 same-executable foundation and relevant G02 shared-service seams. Read FRONTEND_SELECTION as the authoritative truth table.

## Current source facts

Main startup first handles an internal brightness helper, then acquires canonical AppDataRoot single-instance ownership before recovery, persistent reads and services. The Windows mutex is keyed to data identity. The verified G03-B branch in `main` returns silent success for a bare duplicate and shows actionable feedback with a nonzero exit for either explicit selector.

Inspect src/main.rs, src/startup.rs, src/platform/app_data.rs, src/platform/single_instance.rs, target_repair/recovery, settings, visibility and shutdown owners. Do not invent a general CLI parser that the repository does not have.

## G03-A — Parse and dispatch contract

Implement deliberate selector parsing consistent with existing entry modes. Reject malformed/conflicting values clearly without initializing the wrong framework. No selector selects egui even if remembered choice is GPUI. Explicit selectors win.

Document which arguments belong to the app/helper and which may be forwarded during restart. Resolve frontend before framework initialization while preserving the data-lock order for operations that access stores.

**Evidence:** actual parser/dispatch tests for truth-table rows and relevant helper-mode preservation. **Accept when:** selection is deterministic and all default/recovery cases have explicit behavior. Commit selector semantics separately from native restart work.

## G03-B — Existing instance and safe recovery

Keep one owner of a canonical data root across both frontends. Define same-frontend repeated invocation consistently with baseline. For explicit opposite frontend, deliver a controlled restart or clear actionable response requiring current owner closure; never silently reactivate/ignore it.

Document recovery for a GPUI process that cannot show a window and for a still-owning unresponsive process. Do not auto-kill unrelated processes or promise crash catch/hot-swap. Use current local instance primitives; add only necessary bounded coordination.

**Evidence:** selected identity tests plus actual sequential Windows launch scenarios. **Accept when:** explicit egui fallback is usable without navigating GPUI, no selector bypasses data/service ownership, and lock-conflict vs startup-failure diagnostics are distinguishable.

## G03-C — Controlled restart with active state

Implement explicit switching through existing settings/chooser conventions. Remember selection only as described by policy; construct explicit relaunch args. Preserve executable path, working directory/data identity, configured context and intentional startup mode.

Resolve unsaved edits and active jobs/recording/automation via established save/finish/cancel semantics. Honor cancel without switching. Close owned services and release the lock before a new process attempts normal ownership. Prevent action arguments, pending effects or completed jobs from replaying during relaunch.

**Evidence:** deterministic restart-plan/state tests plus native cancel, successful switch and failed-launch recovery scenarios with synthetic documents. **Accept when:** data and operation outcomes are correct and recoverable; unimplemented feature-specific shutdown hooks remain explicitly assigned pending obligations until the feature port.

## G03-D — Cross-frontend configuration and presentation state

Keep canonical stores/formats and application identity shared. Separate egui widget memory from GPUI-specific view memory. Preserve meaningful theme/font/tool/geometry preferences and unknown configuration data retention.

Exercise representative egui→GPUI→egui save/edit/reopen round trips. Verify bare launch remains egui after an explicit switch and remembered GPUI choice. Every later persisted feature adds its own meaningful fixture to this framework.

**Accept when:** shared identity/persistence and framework-local memory rules are implemented, representative round trips succeed, and both startup alternatives from the same artifact remain correct.

## Verification and completion

Use narrow parser/instance/settings/restart tests; native Windows evidence is mandatory for actual process/lock/window behavior. No full nextest or blanket whole-command timeout. Reuse G01 binary evidence when source changes do not invalidate it.

Review shutdown/side-effect/data ownership and truth-table compliance. Goal acceptance does not claim every future unsaved editor is already ported: feature-specific obligations remain visible and close at their owners/G25. Continue G04 and the full GUI program.
