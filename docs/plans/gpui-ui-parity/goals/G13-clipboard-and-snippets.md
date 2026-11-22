# G13 — Clipboard history, Clipboard Modify, snippets, and temporary files

**Status:** Pending. **Owner:** G13 implementer with clipboard/runtime/persistence owners. No clipboard access, test, or source edit occurred in drafting. Work remains on the single migration branch.

## Objective and dependencies

Recreate every clipboard and snippet interface in GPUI Kit while retaining the latest native clipboard fixes, intentional read/write timing, source authority, undo, history, templates, and saved data. The GUI port must not replace the proven clipboard service with a convenient toolkit call that changes behavior.

Depend on G00–G04 and G05 routing; G06 owns temporary-file alias management and settings-shell registration. G13 owns temporary-file content creation, snippets/template prompts, and specialized clipboard/snippet controls. Paste Sequence authoring/playback belongs to G21. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Baseline `f7bc610a86bcad1c14b69dce65b1d804df1b7934` is specifically the latest clipboard-read correction.

| Responsibility | Existing source |
|---|---|
| Clipboard GUIs | [src/gui/clipboard_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/clipboard_dialog.rs), `src/gui/clipboard_modify_dialog.rs` |
| Native read and service | [src/platform/protected_clipboard.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/platform/protected_clipboard.rs), `src/clipboard_modify/{clipboard,coordinator,runtime,watch,executor,store}.rs` |
| Transform configuration | `src/clipboard_modify/{catalog,model,parser,pipeline,config,migrate,help}.rs`, `src/commands/handlers/clipboard_modify.rs` |
| Snippets/tempfiles | `src/gui/{snippet_dialog,snippet_prompt_dialog,tempfile_dialog}.rs`, `src/plugins/{snippets,snippet_template,tempfile,clipboard}.rs`, `src/actions/{snippets,tempfiles,clipboard}.rs` |

The current bounded Clipboard Modify native read has a 750ms budget and a maximum of two read workers, with private-format checks and classified failures. Preserve its established guarantees and chosen backend. The GUI tracks session/source/mutation revisions, request IDs, source authority, and owner-aware commit/undo completion.

## Per-GUI scope

Clipboard history includes browsing, selection, editing, save/copy, removal and confirmation, stale-entry handling, statuses, and native/docked host modes. Clipboard Modify has six verified sections: Modify, Templates, Saved Pipelines, Manage Templates, Manage Pipelines, and Help. Include pipeline-stage controls, arguments, source loading/reloading, preview/cancel, errors, Apply, Apply and Close, Copy Result, persistence, and undo routes.

Snippets include collection editing, aliases, template parsing/validation, preview, deletion decisions, and the complete parameter prompt companion with its field order, validation, keyboard routes, copy/run semantics, and cancellation. Temporary-file content creation includes alias/text inputs, validation, creation feedback, and cancel. Inventory every specialized setting and cross-feature invocation; existing privacy behavior is the contract, not an invitation to add new policies or features.

## Bounded checkpoint packets

### A — Protect clipboard authority and shared operations

**Owner:** G13 with the clipboard-service owner. Map reads, previews, commits, copies, undo, watcher notifications, and history writes to shared operations. Retain bounded worker behavior and failure classification. Extract GUI-only transitions without moving toolkit clipboard objects into core contracts.

**Invariants:** Render/rebuild does not add clipboard accesses. A late load cannot replace a user-edited draft; a stale preview cannot authorize a write. One accepted operation produces one intended mutation/history outcome. Private formats and current diagnostic redaction remain intact.

**Evidence and acceptance:** Contract diagrams, fake-backend counters, and focused existing runtime/protected-read regressions. **Commit boundary:** Neutral adapters and old-path preservation, including the pinned clipboard fix.

### B — Complete clipboard history and all Modify sections

**Owner:** G13 frontend implementer. Recreate history editing/removal and the complete Modify navigation, forms, pipeline management, preview, feedback, and commit actions. Preserve exact labels and enabled states; support long Unicode text without replacing authoritative content with truncated previews.

**Invariants:** History edits/deletion bind to the expected entry/snapshot, not a moving list index. Source reload, typing, pipeline changes, and preview completion invalidate the same authority as the baseline. Apply and Copy Result retain their distinct effects.

**Evidence and acceptance:** Controlled clipboard fixtures cover available/unavailable/private inputs, timeouts, user edit during read, stale preview, apply failure, undo, cancel, and every section's persistence. **Commit boundary:** Complete clipboard GUIs plus scoped regression evidence; do not claim Windows behavior from fake-backend tests alone.

### C — Complete snippets, template prompts, and tempfile content

**Owner:** G13 frontend implementer. Recreate snippet list/editor/preview, alias validation, remove confirmation, template prompt fields and focus order, copy/run/cancel, and tempfile content creation. Use existing parsers, expansion logic, writers, and action routing. Register custom settings through G06's shell contracts.

**Invariants:** A stale snippet edit cannot overwrite a newer entry; prompt values belong to one template generation. Tab/Enter/Escape follow the existing prompt contract. Cancel creates no file and no unintended clipboard write. Rebuilding a prompt does not reset typed values or required-field errors.

**Evidence and acceptance:** Fixture snippets cover valid/invalid templates, required/default fields as supported, duplicate aliases, removal cancellation, and stale prompts; temporary directories verify exact file content and names. **Commit boundary:** Complete snippet/prompt/tempfile content family and focused evidence.

### D — Validate owner lifetimes and native integration

**Owner:** G13 acceptance owner. Exercise hidden-root completion, host migration with pending reads/previews, close/reopen, rapid repeated actions, foreground restoration, and fallback restart. Integrate actual launcher, settings, and later Paste Sequence/widget entry routes without widening scope.

**Invariants:** Retired sessions never receive new mutations; worker completion wakes the actual owner. Clipboard monitoring/services remain single-owned across frontend choice. **Evidence and acceptance:** Windows read/write behavior, native focus records, before/after stored data, and exactly-once traces. **Commit boundary:** Final family-specific fixes and acceptance; unresolved later entry routes stay pending in G25.

## Focused validation

Existing cases include `tests/clipboard_persistence.rs`, domain `clipboard_modify_config.rs`/`clipboard_modify_runtime.rs`, plugin Clipboard Modify parser/help/settings/plugin cases, and standalone `snippets_plugin.rs`/`tempfile_plugin.rs`. Domain cases use `--test domain`; plugin cases use `--test plugin_queries`. List a nonzero selection before, for example, `cargo nextest run -p multi_launcher --test domain -E 'test(clipboard_modify_)'`; narrow further to affected operations. Relevant library filters include `test(clipboard_modify) | test(protected_clipboard)`, but discover/bound the selection rather than reflexively running everything. Preserve the current Windows test runner configuration. Native clipboard ownership/timeout behavior and GPUI text interactions require their own evidence.

## Exit and continuation

All six Modify sections, history, snippets, prompts, temporary-file content, specialized settings, and host modes have complete owned evidence. The latest clipboard-read correction and existing privacy semantics remain protected. No egui prompt or dialog is opened by GPUI. Continue G14 and the remaining program; keep cross-feature routes and final live-session acceptance open until G25–G26.
