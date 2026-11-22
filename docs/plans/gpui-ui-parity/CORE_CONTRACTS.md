# Shared engine and frontend contracts

Status: architectural requirements and inspected starting points; concrete extraction decisions belong to G02 and feature checkpoints.

## Reuse the existing architecture

Inspect `src/commands/{model,parser,bus,host,outcome}.rs` and handlers, `src/gui/command_host.rs`, `src/gui/mod.rs`, `src/main.rs`, `src/plugin.rs`, domain modules, `src/settings/` and `src/persistence/`. The current application already has meaningful boundaries. Extend them where sufficient.

Do not assume a command host is complete merely because it implements a trait: inspect default methods and enumerate every routed command/intent. Do not assume a plugin is frontend-independent: its settings or GUI callback can require `egui::Ui`.

## Required ownership map

| Concern | Authoritative shared owner | Per-frontend responsibility |
| --- | --- | --- |
| Query parsing/ranking/results | Existing search/plugin/query domain, adapted as needed | Editable field, list, selection, navigation and display |
| Action execution | Typed commands/actions and existing execution service | Translate input once and display outcome |
| Documents/calculation/validation | Existing feature-domain types and validators | Editor projection, caret/selection, invalid-input feedback |
| Canonical data | Existing settings/store/persistence/recovery owners | Display draft fields; invoke save/apply/cancel |
| Jobs | Feature services with request identity, cancellation, outcome ownership | Progress/status, view subscriptions, relevance checks |
| Hotkeys/hooks/watchers/indexing | Deliberate application/native service ownership | Selected frontend handles presentation requests |
| UI/window intent | Framework-neutral semantic request/outcome | Window creation, focus, hide/show, geometry and overlay adapter |
| Presentation memory | Separate frontend-specific namespace | egui widget state or GPUI entities/scroll/focus |

Do not prescribe a new crate boundary solely for aesthetics. First establish typed ownership in the existing layout; extract crates only when dependency isolation has a concrete benefit.

## Contract rules

Use meaningful domain types. Keep egui contexts, Ui objects, viewport IDs, textures, GPUI entities/views, element IDs and framework input events out of shared public application contracts. Windows-native types may remain in deliberately platform-specific services; UI independence is not a requirement to make the Windows application platform-neutral.

Avoid duplicating models that independently mutate the same document. Presentation projections can cache derived values, but they require revision/invalidation rules and may not become a second source of truth.

Existing framework-independent Windows presentation helpers, including passive tooltips and native capture/overlay backends, may retain their established owner and thread lifetime. Verify that their GPUI-facing adapter does not construct egui state and preserves no-activate, hit-testing, cleanup and data semantics. Port egui controls that operate these helpers; do not rewrite a proven native substrate solely to change its label.

Jobs need stable request/document/session identity. Specify who owns cancellation and completion when a GUI closes or is recreated. Reject late/out-of-order results. Replayed delivery of one accepted action must not duplicate its writes or launch operations. Preserve the baseline behavior of two deliberate user invocations rather than suppressing legitimate repeated actions. Keep progress observable without adding a permanent busy repaint/poll loop.

Preserve domain limits, permissions, confirmations, data recovery, history/private-data policy and exactly-once operation semantics. Do not move Windows callbacks onto an unsupported thread or change COM apartment behavior accidentally.

## Extraction ledger

For each extraction, record the original owner/callers, new shared owner, public contract, old references removed, egui adapter change, intended GPUI adapter, test targets, and relevant baseline behavior. Before the GPUI path exists, validate core plus egui and leave GPUI acceptance pending.

Compare meaningful outcomes with baseline fixtures or known intended behavior. Agreement between two rewritten adapters alone cannot detect a shared core regression.

## Evidence required before final acceptance

Both frontend paths invoke the same operations; no duplicate core implementation or default no-op route remains. GPUI can execute all required operations without constructing LauncherApp/egui runtime. egui can execute all its operations without constructing GPUI runtime. Data/save semantics agree. Service ownership, task cancellation and lifecycle have relevant focused/native evidence.
