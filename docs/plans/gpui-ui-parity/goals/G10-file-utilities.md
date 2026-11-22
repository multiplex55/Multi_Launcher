# G10 — Archives, Duplicate Finder, Folder Size, and File Lock

**Status:** Pending. **Owner:** G10 implementer, using the existing archive/scan/native diagnostic services. No test or implementation work has occurred. Stay on the single migration branch.

## Objective and dependencies

Recreate four distinct file-tool families in GPUI Kit with their complete controls, review states, error handling, and native window modes. Keep each tool's semantics intact. Shared list or status components are useful, but they must not collapse different concepts such as a same-size candidate, a verified duplicate, or an authoritative extraction plan.

Depend on G00–G04 and G05 entry routes; coordinate shared file actions with G07/G11. Hash Generator belongs exclusively to G04 and is not reimplemented here. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Pinned baseline: `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| Family | Existing GUI and shared source |
|---|---|
| Archives | [src/gui/zip_extractor_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/zip_extractor_dialog.rs), `src/archive/{catalog,creation,model,plan,session,writer,provenance}.rs` |
| Duplicate Finder | `src/gui/duplicate_finder_dialog.rs`, `src/gui/duplicate_finder_smoke.rs`, `src/duplicate_finder/{discovery,content,model,session}.rs` |
| Folder Size | [src/gui/folder_size_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/folder_size_dialog.rs), `src/folder_size/{scan,model,session,treemap}.rs` |
| File Lock | `src/gui/file_lock_dialog.rs`, `src/file_lock/{mod,session,windows}.rs` |

Source inspection confirms the ZIP interface has **Extract and Create modes**, separate source/destination/member revisions, asynchronous pickers, frozen preview tokens, progress/report states, and selection. Folder Size has projection/treemap caching and generation checks. File Lock rejects stale actionable results. G00 must expand every child and optional mode.

## Per-GUI scope

Archives include browsing/filtering/selecting archive entries and folders, destination editing, preview/confirmation, progress/cancel/report, plus creation input files/folders, member-name editing/removal, validation, output selection, and creation status. Preserve current warnings and partial-result reporting.

Duplicate Finder includes root/discovery state, candidate group stages, content verification, groups/members, filtering and actual supported actions. Folder Size includes scan status, sort/filter/drill-down, table and treemap, selected-directory navigation, and reveal/copy routes. File Lock includes source input/picker, check progress, result/diagnostic presentation, and the exact actions the current diagnostic supports. Do not add deletion, forced unlock, or process termination merely because these are common elsewhere.

## Bounded checkpoint packets

### A — Inventory contracts and create shared presentation adapters

**Owner:** G10 with the relevant domain owners. Record per-tool entry routes, stable identities, scan/job generations, status distinctions, action prerequisites, and picker ownership. Extract UI-bound state transitions only where necessary. Reuse common Kit list/status/confirmation controls without making a universal file-tool engine.

**Invariants:** UI indices never become filesystem authority; generation/source revision guards remain intact. Changing mode, root, source, or destination invalidates the same pending authority as in egui. **Evidence and acceptance:** Four distinct contract tables and selected existing domain/GUI checks. **Commit boundary:** Required adapters and egui regression preservation.

### B — Complete archive extraction and creation

**Owner:** G10 frontend implementer. Recreate both modes and all pickers, selections, member editors, validation, confirmation, progress, cancellation, and outcome details. Preserve source/destination/selection revisions and preview-token binding. Render full accessible status while keeping long member paths bounded visually.

**Invariants:** A late picker cannot change another mode/session. Confirmation applies to the reviewed plan; partial publication, cleanup failure, and provenance warnings remain visible. Creation is not omitted because the current module name says “extractor.”

**Evidence and acceptance:** Disposable archive fixtures cover selection, rejected paths, changed destination, creation edits, canceled pickers, failed/partial jobs, and success outputs. Compare outputs through the retained engine. **Commit boundary:** Full archive GUI with targeted tests and native host evidence.

### C — Complete Duplicate Finder and Folder Size

**Owner:** G10 frontend implementer. Recreate the staged duplicate workflow and the folder table/treemap workflow, including keyboard navigation, drill-down, result identity, all supported row actions, and scan cancellation. Reuse shared scan services and cached projections.

**Invariants:** Same-size candidates are not presented as content-verified duplicates. A canceled scan does not keep changing its frozen report. Folder Size sort/filter/resize updates projections without triggering unnecessary scans. Zero-total sizes have the baseline share semantics.

**Evidence and acceptance:** Controlled directory trees cover equal-size unequal content, verified duplicate groups, nested directories, Unicode/long paths, cancellation, stale generations, and zero totals. **Commit boundary:** The two complete GUI families with separate accepted rows and bounded-render evidence.

### D — Complete File Lock and close cross-tool native evidence

**Owner:** G10 frontend/native acceptance owner. Implement diagnostics and existing actions, then exercise every tool's supported host modes, hidden-root completion, close/reopen, external actions, and source changes during pending work. Revisit shared adapters changed by these tools.

**Invariants:** A stale ready result cannot authorize a later reveal/action; failed checks are not actionable success. Native operation failures remain diagnosable. **Evidence and acceptance:** Windows diagnostic fixtures, exact action-target traces, keyboard/focus recordings, and row-level comparison records. **Commit boundary:** File Lock plus final family-scoped interaction fixes and evidence.

## Focused validation

Verified inline cases include `folder_size_treemap_cache_tracks_projection_directory_filter_and_size_without_scans`, `folder_size_dialog_projection_sorts_drills_and_rejects_stale_generation_selection`, `folder_size_dialog_zero_total_has_no_share_and_cancel_status_is_frozen`, and `file_lock_ui_stale_ready_reveal_is_retired_and_failed_check_is_not_actionable`. `src/duplicate_finder/tests.rs` exists; discover the exact relevant tests and archive/picker cases before execution. List a nonzero targeted `--lib` selection, run only affected cases, and use disposable directories/archives. Native lock detection and window focus require Windows evidence; host-independent tests cannot claim those results. Do not run the full repository suite for each family.

## Exit and continuation

All four families, both archive modes, their children, specialized settings, and assigned presentation modes are implemented and evidenced. Any missing platform diagnostic remains blocked honestly. No unsupported operation has been added to enlarge the scope. Continue G11 and retain cross-tool actions and broad performance/package checks for their actual owners in G25–G26.
