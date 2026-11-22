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

### G02-A source map for the first extraction

| Path | Starting owner and observable behavior | Extraction boundary |
| --- | --- | --- |
| Process entry | `src/main.rs` handles the private brightness helper, canonical data-root mutex, recovery/preload, settings and logging before activating plugins, index or GUI services. eframe construction and root capture live in `spawn_gui`. | Keep helper, data identity and recovery common. Select one runtime. Extract a process service only when its alternate caller needs it. |
| Typed routing | `commands::CommandBus` parses and dispatches typed invocations to handler-specific host traits. `commands::handlers::hash_generator` calls `DialogCommandHost::open_hash_generator_dialog` and leaves query, visibility and history unchanged. | Keep real command and handler ownership. Account for the Hash Generator host default, which currently can return without opening a tool. An installed GPUI route must deliver a concrete present/focus intent and use the real tool. |
| Outcome consumption | `gui/command_host.rs::apply_command_outcome_with_root_policy_guarded` owns egui application of query updates, pending query, result invalidation/replacement, search, visibility revision, focus/restore/caret, activation, history, favorite log and toasts. | Preserve each field and the order of relevant state changes. Apply the same out-of-date visibility protection in the alternate frontend before sharing the broader path. |
| Hash computation | `src/hash_generator.rs` owns request identity, algorithms, exact bytes, streaming file reads, validation, cancellation and progress. `HashGeneratorDialogState` still owns the source/session draft, active request generation, result publication, compare and clipboard feedback. | Move the semantic session to the existing hash domain owner and keep the egui adapter’s host intent and viewport concerns local. Both frontends consume the same session and job operations. |
| Hash hosting | `HashGeneratorWindowHost` and `HashHostActions` use presentation generations and egui pass/viewport identities to accept actions. File Search and Downloads Triage provide typed paths. | Keep rehosting, modal focus, file picker parenting and stale host actions in the host adapter. A dock/popout switch retains the semantic session. End the session only on tool close or shutdown. |
| Job publication | Hash worker wakes its caller from `HashJob`. The dialog keeps a retiring worker in the slot until the terminal event is polled, then publishes only for the matching request and generation. | Preserve one job slot and nonblocking retirement. Use framework-local wake on actual events. Closing a window cannot publish an old result into a replacement session. |

G02-B starts with the two ownership changes in the Hash case: move semantic session state and operations, then migrate the egui adapter and directly affected tests. Verification covers the existing `hash_generator` library filter, including lifecycle, typed routing and file-origin behavior. See [Hash Generator](gui/hash-generator.md) for baseline criteria. GPUI path and native evidence remain G02-D/G04 work.

G02-B now owns the semantic state in `src/hash_generator/session.rs::HashSession`. `HashGeneratorDialogState` delegates those operations and retains window/popup state and accepted egui effects. The session exposes the clipboard snapshot's byte count for presentation. See the [G02-B review](reviews/G02-B-hash-session.md) and [validation log](VALIDATION_LOG.md) for the targeted egui and shared-engine evidence.

### G02-C/D Hash wake and typed presentation seams

- The shared Hash worker has one latest progress snapshot and one terminal slot. Initial and rate-limited progress wakes follow snapshot publication and run outside the progress lock. The final wake exposes the terminal result or sender disconnection. egui keeps its existing job-only cadence. GPUI awaits a coalesced one-bit signal with a weak root reference, and the root keeps polling the retained session when its utility has closed and a job is retiring.
- The existing typed `HashGeneratorCommand::Open` handler uses a fallible presentation callback. The egui `CommandBus` preserves its panel ensure-and-focus and default success outcome. A host missing the window capability returns an error. GPUI uses the same handler for its keyboard and button entry and backs the Kit tool with the shared `HashSession`. G05 owns the general `hash` query and result activation path.
- Native evidence includes the same `abc` SHA-256 result, retained reopen, stale digest invalidation after editing and a fresh session after close. See `G02-C-D-GPUI-HASH` in the [validation log](VALIDATION_LOG.md). Full source modes, comparison, clipboard, native file picker and presentation remain with the [G04 packet](goals/G04-design-system-and-pilot.md).

## Evidence required before final acceptance

Both frontend paths invoke the same operations; no duplicate core implementation or default no-op route remains. GPUI can execute all required operations without constructing LauncherApp/egui runtime. egui can execute all its operations without constructing GPUI runtime. Data/save semantics agree. Service ownership, task cancellation and lifecycle have relevant focused/native evidence.
