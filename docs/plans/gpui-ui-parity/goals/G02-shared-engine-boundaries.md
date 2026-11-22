# G02 — Extract authoritative operations without changing egui behavior

**Status: In implementation.** The [source ownership map](../CORE_CONTRACTS.md) specifies the first G02-B extraction. The existing egui behavior remains the baseline. Shared-session verification and GPUI routing are pending.

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective

Make the minimum useful application state, command and service seams independent of either UI, while keeping the existing egui UI working. The goal establishes a shared-engine pattern that each feature port extends; it does not mandate a wholesale up-front rewrite.

Dependencies: G00 ownership map and G01 runtime findings. This goal’s accepted extraction checkpoints may precede the corresponding GPUI feature; record those GPUI criteria pending.

## Source anchors and hazards

Inspect src/commands/model.rs, parser.rs, bus.rs, host.rs, outcome.rs and handlers; src/gui/command_host.rs and LauncherApp; plugin contracts/editor; settings/persistence; startup/services; visibility and utility host/session logic. Many existing domain modules are already useful.

CommandHost defaults can silently do nothing. Outcomes include focus/query/caret/history/notifications alongside domain results. The GPUI adapter must handle every relevant semantic outcome, not only the main action.

## G02-A — Operations, state and capability inventory

Map current command requests/outcomes, model ownership, mutable stores, worker/service owners and presentation intents. Identify actual egui/LauncherApp dependencies and the callers that must migrate.

Define narrow typed contracts using existing command/domain types where sufficient. Specify stable action/document/job/session identity and error/cancel semantics. Establish a command/intent coverage table including default no-op methods and feature routes.

**Accept when:** the next extraction has a real owner, explicit callers/invariants and selected tests, and the architecture can support both adapters without shared UI objects. Do not require every GUI’s internal implementation to be redesigned first.

## G02-B — First real operation extraction through egui

Move an operation required by the pilot/startup into its authoritative existing domain/application service. Migrate the egui caller to it and retain intended outcomes. Remove the old duplicated path after callers migrate. Keep egui Ui/Context, viewport IDs and GPUI entities out of shared public contracts.

Capture baseline expected results before refactoring. Preserve serialization/defaults, validation/limits and side effects. Ordinary helper names/module placement are implementer decisions, not additional user questions.

**Evidence:** focused core tests, meaningful affected egui path and source review. **Accept when:** egui behavior is unchanged through the new owner and no parallel source of truth remains. GPUI evidence stays pending until its actual route exists.

## G02-C — Service lifetime and asynchronous publication

Separate process/application services from egui window construction where needed. Preserve one owner for hotkeys/hooks, clipboard, indexing/watchers, timers, automation and any relevant feature jobs. Maintain supported thread/COM boundaries.

Replace UI-specific wake destinations with semantic subscriptions/adapters. Define cancel/close/reopen behavior, stale-generation rejection, task retirement and notification relevance. Retain utility host session/accepted-action protections without leaking egui frame/pass machinery into core.

**Evidence:** focused lifetime/cancellation/late-result cases and native checks only for changed OS ownership. **Accept when:** one action produces one intended effect, hidden/closed windows do not cause duplicate services or stale writes, and idle behavior does not depend on constant repainting.

## G02-D — Two adapter contract and coverage proof

Make the egui adapter and initial GPUI adapter invoke the same pilot/launcher operations. Enumerate all intended command/outcome coverage, marking future feature adapters pending rather than allowing a default no-op to count.

Extract and document plugin/domain contracts needed to support alternate settings/GUI presentations. Do not pretend a loaded plugin search backend means its egui settings renderer has been ported. Feature-specific callbacks are completed by their owner goals.

**Accept when:** at least one real operation has shared authoritative ownership exercised through both adapters, egui core tests remain meaningful, and the future extraction path is documented with no universal widget API or hidden egui engine.

## Invariants

No duplicate file/calculation/clipboard/command/persistence implementation. No new GPUI-centric core entity owner required by egui. Preserve data identity, service shutdown and Windows platform assumptions. UI independence does not require platform independence.

## Verification, review and continuation

Use affected command/domain/integration targets and exact filters from VALIDATION_STRATEGY; adapt source-structure tests to real new ownership while preserving invariants. A broader named union is justified only by actual consumers. No repeated full suite.

Review each materially shared extraction and cumulative boundary before goal acceptance. Continue G03/G04; feature goals incrementally extend this same engine, avoiding a second “new core” project.
