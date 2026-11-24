# G11 — File Operations Center, Downloads Triage, and Explorer Tabs

**Status:** In implementation. **Owner:** G11 implementer with shared file-transfer and Explorer integration owners. The first Downloads Triage increment supports the G04 Hash dependency. [Current checkpoint](../gui/downloads-triage.md#first-gpui-inspect-checkpoint)

## Objective and dependencies

Recreate the current coordinated file workflows in GPUI Kit, including review decisions, queues, live progress, receipts/history, recovery, Downloads classification/actions, and saved Explorer folder-list authoring. Preserve the authoritative job services; closing or moving a window must not accidentally duplicate or abandon an operation.

Depend on G00–G04 and G05 routing. Coordinate real handoffs with G08 Diff, G09 rename/replace, G10 archives/diagnostics, and G06 shared settings/recovery. This goal owns its specialized controls; G06 owns their shell registration. Apply [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Pinned baseline: `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| GUI | Current UI/runtime/domain source |
|---|---|
| File Operations Center | [src/gui/file_operations_center_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/file_operations_center_dialog.rs), `file_operations_center_runtime.rs`, `src/file_transfer/{plan,jobs,control,execute,journal}.rs`, `src/file_transfer/receipts/` |
| Downloads Triage | [src/gui/downloads_triage_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/downloads_triage_dialog.rs), `downloads_triage_runtime.rs`, `src/downloads_triage.rs`, `src/downloads_triage/store.rs`, `src/file_transfer/downloads_triage/` |
| Explorer Tabs | `src/gui/explorer_tabs_dialog.rs`, `src/explorer_tabs/{folder_lists,identity,operation,backend,native_uia,windows}.rs`, `src/plugins/folders.rs` |

Full dialog source reads confirm typed draft/review tickets, job IDs, recovery tokens, Downloads session/scan-generation selection, and Explorer draft/editor incarnation. Those are current semantics to retain; GPUI host mechanics will be new adapters.

## Per-GUI scope

**File Operations Center:** Copy, move, duplicate, file/folder/destination pickers, review and per-node/group choices, queue/start/reorder, pause/resume/cancel, job details, history, retry/undo recovery confirmations, and all status/error states. Preserve every existing option such as verification; enumerate exact controls during G00.

**Downloads Triage:** Root selection and Windows Downloads route, refresh, destination suggestion, search, reviewed/unreviewed/incomplete/large/installer/archive/same-size filters, type/modified/size/origin grouping, sorting/pagination, selection, review marking, open/reveal, rename, copy/move, extraction, comparison, inspection, and recycle confirmation. Preserve partial-versus-full extraction feedback.

**Explorer Tabs:** Saved folder-list selection/new/edit/save/delete/reorder/run, unsaved-change decisions, saved-folder picker with query/refresh/selection, cancellation, and actual native execution feedback. “Window requested,” “creation unconfirmed,” and “new window identified” are distinct current states; do not turn an attempted launch into verified success.

## Bounded checkpoint packets

### A — Extract view contracts around the existing services

**Owner:** G11 with the service owners. Map each GUI intent to its authoritative runtime operation and completion. Preserve ticket/revision validation, persistence acknowledgements, receipt identities, and ownership of long-running jobs. Replace egui wake/host handles at the adapter boundary without copying queue or recovery logic.

**Invariants:** UI closure and job cancellation remain separate where the current behavior separates them. A late picker cannot mutate a newer draft; a review choice cannot target a different plan. **Evidence and acceptance:** Service/GUI ownership diagrams, action-contract tables, and focused old-path tests. **Commit boundary:** Required neutral contracts and egui adaptations.

### B — Complete File Operations Center

**Owner:** G11 frontend implementer. Build draft inputs, review tree/choices, queue controls, progress, detail/history views, and recovery decisions. Keep current data and receipts authoritative. Give stable identities to jobs and reviewed nodes so sorting/reordering does not retarget controls.

**Invariants:** Queueing or confirming once creates one intended job; pause/resume/cancel do not acquire altered meanings. Recovery is bound to the displayed token/receipt. Failure/partial completion remains visible and distinct from success.

**Evidence and acceptance:** Disposable file trees cover review conflicts, grouped decisions, queue reorder, pause/resume, cancellation, completed/failed jobs, and eligible retry/undo. Compare outputs and history with baseline expectations. **Commit boundary:** Complete operations-center GUI and scoped evidence.

### C — Complete Downloads Triage and its handoffs

**Owner:** G11 with file-tool handoff owners. Recreate classification/grouping/filtering, reviewed state, selection, pagination, actions, and recycle decision. Wire extraction/comparison/inspection/transfer to the real shared operations and the owning GPUI surfaces as they land.

**Invariants:** Refresh retires stale selection authority; reviewed and extraction markers persist by the existing identity rules. Group/sort changes cannot recycle or transfer a different file. Cancel leaves the selection's files unchanged.

**Evidence and acceptance:** A disposable Downloads-like corpus exercises filters, renamed/deleted entries, stale scans, action payloads, and explicit recycle cancellation/acceptance using existing semantics. **Commit boundary:** Complete Triage UI; handoffs awaiting other goals stay pending integration rows, not accepted mock routes.

### D — Complete Explorer Tabs and native lifecycle evidence

**Owner:** G11 native/frontend implementer. Port folder-list authoring, picker, save/discard/cancel decisions, run/cancel feedback, and host changes. Validate the existing native Explorer backend on Windows; do not replace it with unverified shell launches.

**Invariants:** Running a saved list reloads the saved identity as the baseline does and rejects deleted/corrupt storage. A stale picker/editor incarnation cannot append folders to another draft. Native evidence is required before reporting identified Explorer tabs/windows.

**Evidence and acceptance:** Source/data round trips plus Windows recordings cover list editing, picker search, external store changes, native execution, cancellation, root-hidden operation, and all three tools' host modes. **Commit boundary:** Explorer GUI/native integration and final family-specific corrections.

## Focused validation

Verified inline tests include `file_operations_center_picker_requires_current_owner`, `file_operations_center_stale_picker_and_close_preserve_draft`, `downloads_triage_dialog_session_retires_selection`, `explorer_tabs_picker_keyboard_navigation_keeps_snapshot_indices`, `saved_folder_picker_search_copy_staleness_and_limit`, and `explorer_tabs_run_reloads_saved_id_and_refuses_deleted_or_corrupt_store`. Discover/list nonzero targeted `--lib` selections and affected file-transfer/receipt cases; do not invent separate Cargo binaries from module paths. GPUI keyboard/confirmation tests and Windows Explorer evidence supplement existing domain coverage. Keep filesystem experiments confined to isolated fixtures and avoid a routine full nextest run.

## Exit and continuation

Each workflow, child decision, specialized setting, and assigned window mode has evidence. Required cross-tool handoffs must eventually pass using the actual GPUI destination; pending targets remain visible in G25. The authoritative job services and stored receipts remain compatible with egui fallback. Continue the text/document goals and later integration rather than stopping after successful file-copy demos.
