# Codex Goal — Launcher and Utility Fixes

**Goal slug:** `launcher-utility-fixes`  
**Companion:** `CODEX_KICKSTART_LAUNCHER_AND_UTILITY_FIXES.md`  
**Approved scope:** user tasks A–H. Keep these task letters in progress reports.  
**Execution contract:** bootstrap M0-A, complete A, B and C first, perform and publish the interim A–C peer review on GitHub, then immediately continue D, E, F, G and H. The interim review is not the end of this Goal.

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

The Goal and Kickstart describe **one shared task branch**. Use a fresh, uniquely named branch for this Goal, one existing checkout, and sequential source writers. Ordinary milestones, the A–C review, and individual agents do not receive separate branches.

**M0-A is the first execution checkpoint and a hard prerequisite to application edits, implementation builds/tests, and M1.**

1. Read the repository's `AGENTS.md`, applicable local instructions, this entire Goal, and the Kickstart. Inspect `git status -sb`, current branch, `HEAD`, remotes/upstream, the index and working tree, and the current queue/execution handoff. These initial checks are read-only. Identify pre-existing changes without modifying or staging them.
2. Resolve the exact immediately preceding completed Goal/category's branch and final commit. Record `parent_branch` and `parent_tip_sha`. For a continuing chain, this exact final tip is the required base even when the checkout currently shows `master` or `main`. Inspect the live handoff, not a remembered SHA. `docs/plans/execution/standalone/productivity-tools-refinement/STATUS.md` is one relevant existing ledger to check if it remains the latest handoff; verify any later records before choosing the parent. A terminal LIMITED handoff does not turn its unverified behavior into accepted behavior; confirm that development has ended and that its final committed tip is the intended predecessor. If records conflict or ancestry is genuinely uncertain, report the precise blocker instead of guessing.
3. Only the first Goal in a genuinely new chain defaults to the up-to-date primary integration branch. Fetch normally when needed to inspect remote state; verify the exact commit before branching. Do not pull, merge or rebase as an automatic preparation step. A safe current-HEAD base for a first task is permitted only when explicitly documented and it neither loses nor incorporates unrelated user work.
4. Choose a unique lowercase kebab-case task name, for example `fix/launcher-utility-fixes`, adding a unique suffix if necessary. Do not reuse a predecessor or already-active task branch as this Goal's new branch.
5. Preserve user changes. Never automatically reset, hard-pull, stash, rebase, merge, squash, cherry-pick, force-checkout, overwrite files, or stage unrelated work to make the transition possible. If switching to the required parent cannot be done safely, stop at that genuine Git blocker rather than choosing another base.
6. Create and switch to the new branch directly at `parent_tip_sha`, for example `git switch -c "$task_branch" "$parent_tip_sha"`. Verify its initial `HEAD` equals the recorded parent. Do not implement on the predecessor, `master` or `main`.
7. Create this branch's own M0-A baseline commit. Prefer a small execution ledger at `docs/plans/execution/standalone/launcher-utility-fixes/STATUS.md`; reuse an authoritative queue ledger instead if one already owns this Goal. An intentional empty baseline commit is acceptable if needed. Suggested subject: `chore: [M0-A] establish launcher utility fixes baseline`. Record `baseline_sha` separately from `parent_tip_sha` and verify that `git rev-parse <baseline_sha>^` equals the required parent. Stage only the intended baseline material.
8. Attempt one ordinary initial upstream push, normally `git push -u origin HEAD` after confirming the remote is the intended repository. Record success or the exact failure. A denied/unavailable push does not block safe local implementation. Preserve commits and continue without bypasses or retry loops. Later checkpoints receive their normal individual push attempts.
9. Keep all implementation checkpoints, interim review corrections, review documentation, and D–H work on this same branch. Commit small coherent increments and attempt non-force pushes. Do not introduce category branches within this Goal. Do not merge, rebase, squash or cherry-pick intermediate branches merely to advance work.
10. At completion, record the current branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint SHAs, actual push state, checkout state and exact `final_tip_sha`. The next Goal must create its child branch directly from this final tip. A local final tip remains the required parent after a failed push; execution on another machine must wait for that ancestor to become available rather than silently use stale remote state. The cumulative chain is intended for a single final integration PR/merge at the authorized integration boundary.

**No Git worktrees:** never run `git worktree add`, `git worktree move` or `git worktree prune` for this workflow. Do not create auxiliary checkouts, secondary clones, agent-specific branches, or parallel branches from a common integration base. Read-only investigation may run in parallel; overlapping edits, branch operations, builds and tests remain coordinated in the single checkout.

**M0-A acceptance:** the new unique branch is active at the correct ancestry; its own baseline directly descends from `parent_tip_sha`; the initial push was attempted and recorded; user changes were preserved; no unrelated work was staged; and no worktree, helper clone or independent side branch was created. Record the baseline hash after its commit, in the next ledger update or the existing external execution record; do not create self-referential commit loops.

## A. Goal / Desired Outcome

Improve launcher interaction, production configuration and specific utility workflows without redesigning the application. The root query must remain usable alongside popped-out dialogs. Search results must stop showing the unwanted edge fade/flicker. A normal release build must use conservative production settings, with less unnecessary idle work. Calculation Pad, Diagram Studio/Viewer, Folder Size and Explorer Tabs must behave as requested in tasks D–H.

### Mandatory priority and continuation

| Order | Work | Required transition |
| --- | --- | --- |
| 0 | M0-A: correct child branch, own baseline and initial push attempt | Begin A only after bootstrap |
| 1 | A: root query interaction and focus | Complete focused verification, commit, continue B |
| 2 | B: root search-result fade/flicker | Complete focused verification, commit, continue C |
| 3 | C: default release configuration and localized idle improvement | Complete C, including the ordinary release build, then review A–C |
| 4 | M3-R: independent peer review of the A–C changes, published on GitHub | Immediately continue D; keep the Goal active |
| 5–9 | D, E, F, G and H, in that order | Complete each bounded checkpoint and continue |
| 10 | Final focused review, verification and committed handoff | Finish only at the complete A–H boundary, or report a genuine blocker accurately |

Do not start D–H implementation before the A–C implementation/review checkpoint. Read-only planning is allowed. Do not turn C into a terminal Goal, freeze the task branch after C, wait for a human review response, ask whether to continue, or require the interim PR to merge before beginning D. A child implementer's instruction to return after one assigned milestone applies to that worker; the parent orchestrator must assign the next milestone and continue the Goal.

## B. Current Codebase Findings

These are source observations and implementation starting points. Reconfirm symbols in the active checkout after M0-A. They do not claim that native behavior or a build has already been verified for this Goal.

| Task | Actual source / symbols | Finding and implication |
| --- | --- | --- |
| A | `src/gui/mod.rs`: `any_panel_open`, `panel_has_root_presentation`, `any_root_panel_open`; `src/gui/render.rs`: `root_navigation_allowed`, `jump_mode_unsupported_panel_open`, `query_focus_ordinary_context`, `query_mouse_focus_available` | Several root input gates use global open-state rather than root presentation. Calculation Pad and Diagram Studio are not explicitly classified in `panel_has_root_presentation`; detached-only states can also lack the ordinary query-focus context. Fixing only one `any_panel_open` call is insufficient. |
| A | `src/gui/calculation_pad_dialog.rs::host_layer_if_root`; `src/gui/markdown_studio.rs::host_layer_if_root`; Diagram Studio's `docked_owner_layer` / `embedded_viewer_layer_if_root`; `src/gui/utility_host.rs` | Authoritative host/root-layer information already exists. Preserve embedded fallback and the current focus-session/viewport-generation machinery. |
| B | `src/gui/render.rs`: shared root-results `ScrollArea::vertical().show_viewport`; `vendor/egui/src/style.rs::ScrollFadeStyle`; `vendor/egui/src/containers/scroll_area.rs` | The vendored egui version supports `ui.spacing_mut().scroll.fade.strength = 0.0`. Its default strength is 0.5. Both result layouts repeatedly request centered selection scrolling; an actual oscillation remains a runtime hypothesis. |
| C | `Cargo.toml`: `profile.fast-dev`, `profile.iteration`, eframe/image dependencies | There is no explicit release profile. `iteration` inherits release. Glow and supported image decoders are deliberate choices, and application/Markdown code uses different image versions. |
| C | `src/gui/hotfolder_runtime.rs`: `DRIVER_IDLE_TICK`, `driver_main`, `HotFolderCapabilityInputs::with_context`, `publish_driver_snapshot`, `shutdown_and_reap`; `src/hotfolder/execution.rs::run_next_ready` | The enabled driver wakes every 80 ms and constructs capability inputs/probes directories before discovering there may be no executable work. Its shutdown currently depends on waking to observe an atomic flag. The lower watcher and main orchestration loop already use event/deadline-driven waiting. |
| D | `src/calculation_pad/session.rs`: `is_saved`, `restore_recovered_source`, `request_close`; `src/calculation_pad/persistence.rs::PadRecoveryWriter::save_now`; `src/gui/calculation_pad_dialog.rs`: `apply_close_request`, `save_recovery_now` | All dirty sources currently enter the close decision. The session already distinguishes a disk-backed baseline from scratch text. Recovery must succeed before a new silent-close path retires the source. The Discard action clears recovery and must not be used as a shortcut. |
| E | `src/gui/diagram_studio.rs`: `show_viewer`, `draw_viewer_body`, `draw_visual_viewer_body`, `draw_generic_canvas`, `draw_canvas`, `PendingViewerEffects`, `viewer_viewport_id` | The read-only viewer has multiple render modes and already supports zoom, middle-button pan and a minimap. Primary dragging can move the viewer without removing those interactions. |
| F | `src/gui/diagram_studio.rs`: `show_impl`, `draw_body`, `draw_visual_body`, `DocumentIntent::ResolvePending` | Mermaid and Visual dirty-decision controls are inside separate content scroll areas. Host presentation controls already sit above the body, providing the appropriate top-level location. |
| G | `src/gui/folder_size_dialog.rs`: `show_scan_status`, `show_folder_rows`, `sorted_child_indices`, `FolderSizeProjection`, `FolderSizeAction` | Details are inline; at most eight issue samples reach the projection. Rows have fixed size-descending ordering and no requested header sorting/filter controls. Selection already uses generation plus original entry index. |
| G | `src/folder_size/model.rs`: `MAX_FOLDER_SIZE_DIRECTORIES`, `MAX_FOLDER_SIZE_PATH_BYTES`; `src/folder_size/scan.rs::SCAN_PATH_BUDGET_BYTES`; `src/file_operations.rs` metadata scanning | The current directory cap is 2,048. The 32 MiB aggregate path allowance assigns only 3 MiB to discovery. The reported 3,145,728-byte error is the discovery allowance, not merely the directory cap. Both layers need consistent bounded headroom. |
| G | `src/file_search/actions.rs::reveal_path`; `src/gui/render.rs` dispatch of `FolderSizeDialogEffect` | Reuse the Explorer reveal helper, routed through accepted dialog effects. Existing OpenPath opens directories; that is not the same action as revealing a selected directory in its parent. |
| H | `src/explorer_tabs/windows.rs`: `WindowsExplorerTabsPort::preflight`, `WindowsShellObserver::observe`, `enumerate_unfiltered_windows`, `window_identity`, `recheck_shell_rows`; `src/explorer_tabs/identity.rs` | The reported text comes from preflight rejecting an incomplete Shell baseline before requesting a window. `WindowIdentityUnavailable` is used at several distinct failure sites. Its index alone does not identify the native cause. The unfiltered baseline prevents mistaking an existing window for a newly created target. |

### Decisions that do not require a questionnaire

Use existing ownership and persistence boundaries. Preserve Remember-off Calculation Pad behavior; the new no-prompt rule applies to remembered, unbound scratch text. Use All / Files / Folders as Folder Size filters. Keep truthful partial-scan reporting. Use an automated independent reviewer and a GitHub PR review comment plus a tracked report for the interim checkpoint. None of these choices requires another user approval.

## C. Approved Functional Requirements

### Task A — Main launcher query remains available

- Any nonmodal popped-out dialog, including Notes and Markdown Studio, must be able to stay open while the user clicks, edits and uses the main launcher query and another plugin.
- A real primary click on the visible, unobstructed query transfers input to the user. Subsequent frames must not steal the caret or redirect typing to an old dialog/Jump owner.
- Cover Jump Mode enabled and disabled, multiple detached windows and relevant dock/undock transitions. Keep the existing query widget, shortcuts, search results and activation paths.
- Genuine occluding modal decisions, an active capture operation and another foreground native window still retain their real ownership; do not simulate input through them. An open detached editor alone is never such a blocker.

### Task B — Localized search-result fade/flicker correction

- Set `ui.spacing_mut().scroll.fade.strength = 0.0` in a UI scope local to the root results before the shared ScrollArea is built.
- Preserve list/grid geometry, styling, highlighting, context menus, mouse selection, wheel interaction and keyboard selection scrolling.
- Change repeated selection-scroll timing only if flicker remains after the fade fix and evidence identifies it as a contributing cause. Do not disable scrolling globally or change vendored egui for this fix.

### Task C — Normal release build and reduced idle work

- Configure `[profile.release]` directly so `cargo build --release` needs no special wrapper, new profile, script or packaging workflow.
- Start with `opt-level = 3`, `lto = "thin"`, `codegen-units = 16`, and `strip = "symbols"`. Preserve unwinding. Thin LTO may add build/link work; report the tradeoff without claiming all metrics improve.
- Preserve effective development/fast-dev/iteration behavior. In particular, add `strip = "none"` to iteration if needed to prevent the new inherited stripping setting from changing it.
- Perform a short feature-usage review, removing only proven unnecessary features/dependencies. No safe removal is an acceptable finding. Preserve supported decoder formats, renderer, accessibility, existing platforms and production panic recovery.
- Avoid Hot Folder capability construction/path probes when no execution is ready, while preserving real execution admission, watcher wakeups, cancellation, pause/disable behavior and shutdown. Avoid republishing unchanged snapshots only if this fits the same small change.
- Build once with the ordinary release command at the end of C and run directly affected checks. No formal benchmarks, broad profiling, distribution validation or comprehensive regression campaign.

### Task D — Calculation Pad remembered scratch close

- With Remember last source enabled and the current source unbound to a saved/opened document, close without Save / Discard / Keep Editing after successfully persisting the latest accepted source.
- Reopening restores that source. A failed recovery write keeps the source open and reports the error; it must not silently lose the draft.
- Dirty sources loaded from or saved to a document retain normal save decisions and external-file conflict protection even when remembering is enabled.
- Preserve existing Remember-off behavior and New/Open decisions. Do not represent recovery as a saved document or clear recovery using the Discard path.

### Task E — Diagram Viewer window dragging

- Primary-button dragging on the viewer's non-control preview surface moves the viewer window in every render mode, including a placeholder/error preview.
- Preserve toolbar buttons, zoom, middle-button pan, minimap interaction and Diagram Studio editing. Dragging the viewer must not edit the graph or change its camera.
- Target only the viewer viewport. Docked/embedded content must never issue a native drag command to the main launcher; use the existing egui-window movement path for that presentation where applicable.

### Task F — Visible Diagram Studio pending-change decision

- Render Save / Discard / Cancel near the top, immediately below or beside the Dock/Pop Out controls, outside the content ScrollArea.
- Apply to Mermaid and Visual documents, in native, docked and embedded presentations.
- Keep one visible decision panel. Preserve save failure, conflict, cancel and transition semantics; small-window/text-scale wrapping must keep the buttons reachable.

### Task G — Folder Size usability and larger bounded scans

- Keep a compact status/subtotal/issue-count summary visible. Put the verbose counters, partial-result explanation and issue details in a collapsible, independently scrollable area with a finite height. Start it collapsed; keep fatal/action errors noticeable.
- Make each displayed data column sortable ascending/descending with a visible active direction. Add an explicit Type indicator or column so File / Folder / Skipped is understandable without relying only on color.
- Provide All / Files / Folders filters for the current folder. Unknown/omitted kinds remain truthful, and are available in All. Sorting/filtering must not change scan totals, original entry identities or the underlying result.
- Right-clicking a row selects that exact row and offers Reveal in Explorer. Resolve its original generation/index at accepted-effect dispatch and report failures. For a normal folder, reveal it in its parent; preserve existing Open behavior separately. Handle a filesystem root sensibly by opening it when it has no parent to select in.
- Increase directory capacity substantially above 2,048 and increase discovery path headroom coherently. Use an initial engineering target of **16,384 directories** and a **64 MiB aggregate path allowance**, with a reviewed partition such as 32 MiB discovery, 8 MiB result paths, 8 MiB cumulative progress paths, three 4 MiB issue-path allowances and 4 MiB for root/bookkeeping. These are bounded path allowances, not a total-RSS guarantee. Adjust that partition only if source accounting requires it and record the final values and rationale.
- Preserve the existing 100,000-entry, depth and issue-count bounds unless a concrete compatibility correction is necessary. Preserve cancellation, partial subtotals, checked arithmetic, reparse/cloud policy, identity validation and root-change handling. Do not promise unrestricted full-volume scanning or change unrelated metadata-scan callers.

### Task H — Explorer Tabs saved-list capability failure

- Fix the failing native identity/observation boundary behind `incomplete Shell baseline: Some(WindowIdentityUnavailable { index: 0 })` for an otherwise supported system. Opening a saved list must progress to the intended verified tab-opening workflow.
- Distinguish the actual failing stage; do not assume index 0 identifies one particular Explorer window or one fixed cause.
- Preserve the complete prelaunch inventory, safe target binding, COM/UIA worker ownership, cancellation/deadlines, ordered saved folders and truthful attempted/verified/not-attempted reporting.
- A genuine unsupported capability or unverifiable target still produces a specific bounded error without creating unrelated windows. H is not complete merely because the error wording changed.

## D. Scope and Non-Goals

The scope is A–H and the requested interim GitHub peer review. Reuse current types and lifecycle mechanisms. Preserve global hotkeys, query semantics, drafts, saved document formats, renderer/decoder support, native window identity, scan safety and platform behavior directly touched by these fixes.

Do not introduce workspaces, a new command bus, a replacement window manager, new production dependencies without demonstrated need, broad GUI redesigns, blanket focus forcing, image-codec removal, dependency migrations, a separate release profile, benchmark infrastructure or repository-wide cleanup. Historical acceptance campaigns and unrelated test/lint failures do not become gates for this Goal.

## E. Technical Implementation Strategy

1. Correct root-versus-detached ownership centrally in the existing launcher/host queries; do not create per-dialog focus hacks. Keep pointer transfer tied to the actual retained `query_input` response and current viewport identity.
2. Remove only the root-results fade first. Keep scrolling policy unchanged unless runtime evidence warrants an additional small correction.
3. Keep C in two coherent checkpoints: manifest/profile review, then a localized Hot Folder idle-work change. Run one ordinary release build after both are present.
4. Freeze the A–C reviewed code by commit SHA, not by ending the branch. Publish the independent report and continue on the same branch.
5. Make D's persistence guarantee part of the close transition. Put E's drag in the existing accepted viewer-effect lifecycle and F's decision in shared host chrome.
6. Make G's sorting/filtering presentation state over stable original indices; cache derived ordering per projection/settings change. Keep shell actions outside speculative/discarded GUI passes. Change scan bounds only in the Folder Size owner and its supplied scan limits.
7. Diagnose H at the actual Win32/Shell identity boundary. If a transient observation race is proven, a bounded fresh observation within the existing deadline is appropriate. If irrelevant windows and actionable Explorer identities must be separated, retain a complete raw prelaunch handle inventory for exclusion and strong identity proof for every actionable target. Never label an incomplete inventory complete or simply skip unknown windows to make preflight pass. Avoid such a type adjustment unless it is needed by the reproduced cause.

## F. Detailed Milestones and Tasks

Every checkpoint below inherits A0, the functional requirements and the verification budget. Verify the affected behavior, inspect the diff, commit the coherent increment and attempt a normal push before advancing. Do not manufacture extra commits for trivial edits.

### M1-A — Task A: restore root query ownership alongside detached dialogs

**Objective / ownership:** make the root launcher query independently usable while utility windows remain open. The launcher owns root interaction; host objects own presentation and viewport identity.

**Files:** `src/gui/mod.rs`, `src/gui/render.rs`, and only the host-query accessors in relevant dialog modules that need correction.

**Implementation:** trace root query enabled state, root-navigation ownership and unsupported-Jump-panel classification together. Replace global-open-state assumptions at the relevant gates with actual root/embedded presentation. Complete Calculation Pad/Diagram classification and use authoritative root-layer methods for Notes/Markdown fallback. Provide the ordinary root query context when only detached windows are open. A real query click must retire conflicting stale query/Jump authority and retain user focus across subsequent frames. Reuse current focus sessions, native/visibility generations and post-dialog revalidation; do not globally change the meaning of `any_panel_open`.

**Behavior / edge cases / invariants:** multiple detached windows, mixed docked/detached hosts, Jump on/off, dock/undock and launcher show/hide must work. Click-to-focus must preserve caret placement, text selection, Unicode typing, autocomplete/history and result activation. Genuine popup/modal/capture occlusion and foreground-window ownership remain protected. Detached editors keep their drafts and can regain focus normally.

**Verification:** extend existing actual-frame helpers such as `prepare_actual_query_frame`, `run_root_frame_for_test` and the `notes_query_mouse_*` helpers. Use a small behavioral table for representative detached Note, Markdown, Calculation Pad, generic utility and Diagram hosts, plus an inexpensive policy check for remaining classifications. Select directly related `studio_usability_query_focus_*`, `notes_query_mouse_*`, `notes_query_activation_*` and `launcher_show_focus_*` cases as needed; do not run all GUI tests. On Windows, detach Notes and Markdown together, edit the root query, launch a third plugin, switch back and verify the drafts and root result activation.

**Acceptance / commit:** the actual query widget is enabled and accepts intentional user input across frames, no detached-only host blocks it, and relevant ownership tests pass. Record any unavailable native evidence honestly. Commit `fix(launcher): [M1-A] keep root query usable beside detached dialogs`.

### M2-A — Task B: remove the root result edge fade

**Objective / files:** remove the unwanted overlay from list/grid results in `src/gui/render.rs` without changing unrelated scroll areas.

**Implementation:** set fade strength to zero in a result-local UI scope before constructing the shared vertical ScrollArea. Preserve its identity, pixel-aligned viewport, virtualization and selection geometry. Check the symptom after this correction. Only if it remains, trace selected-entry centering and gate redundant requests by real selection/result/layout transitions while retaining offscreen selection visibility.

**Behavior / edge cases / invariants:** selected bottom-visible entries must remain visually stable while scrolling/resizing; list/grid keyboard and mouse behavior must remain correct. Do not remove automatic scrolling or globally change styling.

**Verification:** use one directly relevant existing grid/list geometry/interaction selection and a focused native visual check when supported. Inspect bottom rows at the configured scale, move selection across the viewport boundary, and use wheel/context menu. A short capture is preferable if it is needed to show temporal flicker. Do not add brittle source-text or pixel-count assertions for a style setting.

**Acceptance / commit:** the local override is active, unrelated scroll areas are unaffected, and touched navigation behavior is preserved. Commit `fix(launcher): [M2-A] disable root result scroll-edge fade`.

### M3-A — Task C: conservative default production configuration

**Objective / files:** put the approved settings in `Cargo.toml`; change `Cargo.lock` only if a justified feature/dependency correction changes resolution.

**Implementation:** add the release profile specified in C. Keep panic unwinding because production paths use `catch_unwind`, including visibility dispatch and workers. Explicitly preserve iteration stripping behavior and its existing optimization/LTO/codegen/incremental settings; preserve fast-dev and dev. Update only the now-stale profile comment. Perform a short, source-backed feature audit using current call sites and the target's Cargo feature graph. Keep intentional image versions/decoders, Glow and existing backend/accessibility behavior; document a no-removal result if none is safe.

**Behavior / edge cases / invariants:** ordinary `cargo build --release` uses the new settings. Do not add target-CPU=native requirements, bespoke release commands or a distribution profile. No dependency/version migration for size alone.

**Verification:** inspect profile inheritance and manifest/lock diff. If features change, check only the affected consumers/formats. The production build is consolidated at M3-B, so this checkpoint may explicitly record that release validation is pending there.

**Acceptance / commit:** manifest changes are coherent, iteration/development settings remain effectively preserved, and any dependency removal has specific evidence. Commit `perf(build): [M3-A] configure the default release profile`.

### M3-B — Task C: eliminate unnecessary idle capability work and build release

**Objective / ownership:** avoid needless Hot Folder filesystem probing/allocation when no job can execute. The existing app-owned driver/executor remain the owners.

**Files:** `src/gui/hotfolder_runtime.rs`; a narrow readiness/lazy-input seam in `src/hotfolder/execution.rs` if necessary. Inspect the lower watcher in `src/hotfolder/runtime.rs` without rewriting it.

**Implementation:** arrange for `HotFolderCapabilityInputs::with_context` and root/destination inspection to occur only when execution needs them. A cheap readiness observation is merely an optimization hint; `run_next_ready` must still enforce fresh admission, leases, rule versions/fingerprints, source validity, pause/disable and throttling. Avoid unchanged snapshot publication if it can be done without a new infrastructure layer. Preserve 80 ms service timing unless an equally small change proves event/deadline parity. Do not replace waiting with indefinite receive while shutdown still relies on the atomic flag and timeout.

**Behavior / edge cases / invariants:** an enabled idle service does less work; newly settled events execute promptly; pause, disable, background monitoring and shutdown still work. Keep activity-driven GUI repaints for recording, screenshots and other real work.

**Verification:** add a focused behavioral check that idle ticks do not build/probe execution capabilities, and select a settled-candidate plus pause/disable/shutdown lifecycle case. Existing anchors include `m10e_driver_reuses_service_and_restores_enabled_intent_without_backfill`, `m10f_disabled_rule_requires_strong_enable_and_moves_only_new_settled_files_across_lifecycle`, and `m10c_fake_ready_execution_dedupes_partial_outputs_and_caps_runs`; choose only what the actual change touches. Run **`cargo build --release` once after C is implemented** and smoke-launch that executable on Windows when supported. A Linux build alone is not evidence that the Windows EXE builds.

**Acceptance / commit:** directly affected tests pass, the normal release build succeeds in the target environment, and the release/idle changes preserve required functionality. Report expected resource benefits without invented measurements. If the Windows toolchain is unavailable, record release verification as environment-blocked rather than passed and follow section I. Commit `perf(hotfolder): [M3-B] avoid capability work while idle`.

### M3-R — Publish the A–C peer review, then immediately continue

**Objective / owner:** the configured reviewer independently checks completed A–C work; the parent publishes the result on GitHub and advances to D. This is an interim checkpoint inside the same active Goal.

**Scope:** the diff from `baseline_sha` to the committed A–C tip, plus directly relevant source and verification receipts. The review does not cover or require implementing D–H yet.

**Tasks:**

1. Record `abc_tip_sha` after A–C code and required available checks are complete. Give the reviewer the actual diff and the A–C acceptance requirements. Review root input ownership, result rendering/navigation, effective profile inheritance, supported features and Hot Folder lifecycle. Identify concrete defects and verification gaps; do not initiate a broad audit.
2. Resolve substantive A–C findings with small same-branch corrective commits and rerun only affected checks. Have the reviewer check corrected findings. Record the exact `abc_reviewed_tip_sha`; never describe an earlier uncorrected commit as the final reviewed code.
3. Write `docs/plans/execution/standalone/launcher-utility-fixes/PEER_REVIEW_ABC.md` (or the equivalent under the authoritative queue directory). Identify the report as an **automated independent peer review**. Include task scope, baseline and reviewed SHAs, reviewer role, severity-ordered findings/dispositions, actual checks/results, native/build limitations and a clear statement that D–H remain in progress. An absence of findings must be stated plainly without pretending it is a human GitHub approval.
4. Commit the report and current status, then attempt the normal push. Reuse the existing PR for this task branch. If none exists and GitHub is available, create **one draft PR for the full A–H Goal** against the intended final integration branch; verify branch/base first. Post a peer-review comment containing the summary, reviewed commit and report link. Use the available GitHub connector or `gh`; with `gh`, place multiline bodies in a temporary UTF-8 file and use `--body-file`. Do not assign/request a named human reviewer, notify unrelated people, merge the PR or create a second A–C integration branch.
5. Record the successful report URL/PR comment URL and published commit. Pin the report link to its commit so subsequent D–H changes do not obscure the A–C review boundary. A local report or a failed push is not GitHub publication. If authentication, network, permission or repository access prevents publication, preserve the report and commits, record `publication_pending` and the exact reason, and continue D–H. Attempt reconciliation at a later ordinary checkpoint if access has changed; do not enter a retry loop or claim the request was fully delivered while it remains pending.
6. **Immediately start M4-A / task D. Do not pause, finish the Goal, wait for a user reply, wait for PR approval/merge, or wait for GitHub CI unrelated to this task.** Keep the same branch and execution ledger active.

**Edge cases / constraints:** do not publish private machine paths, secrets or uncontrolled logs. A native validation limitation is reported as such; it is not silently converted into a pass. If an actual A–C implementation defect prevents safe continuation, use the bounded blocker procedure rather than describing review publication as completion.

**Acceptance / commit:** the independent review is complete, substantive findings are resolved or precisely blocked, the tracked report is committed, publication succeeded or is explicitly pending, and the next active checkpoint is D. Suggested commit: `docs(review): [M3-R] publish interim A-C peer review`. Publication-pending is an external delivery limitation, not permission to stop implementation after C.

### M4-A — Task D: close remembered scratch calculations safely

**Objective / ownership:** distinguish recovery-backed scratch close from saved-document decisions in the session/dialog transition boundary.

**Files:** `src/calculation_pad/session.rs`, `src/calculation_pad/persistence.rs` only if needed, and `src/gui/calculation_pad_dialog.rs`.

**Implementation:** use existing `remember_source` plus `!session.is_saved()` to select the special close policy. Flush the latest accepted source through `save_recovery_now` / `PadRecoveryWriter::save_now` before retiring the session. Introduce a small explicit session close policy/helper if needed; do not fake a disk baseline or use `decide_pending(Discard)`. Preserve the accepted-pass lifecycle so same-frame edits are included. On write failure retain the draft and cancel native close as required.

**Behavior / edge cases / invariants:** remembered scratch closes quietly and reopens unchanged; named/opened documents prompt only when dirty; recovery failure, external file changes, Remember-off and New/Open semantics stay safe. No schema migration or evaluator rewrite.

**Verification:** a scratch close/reopen case including a same-frame final edit, recovery write failure with draft retained, and dirty saved/opened-document close/conflict. Adapt the old remembered-scratch expectation in `accepted_transition_intents_preserve_or_clear_recovery_at_lifecycle_boundaries` without weakening its other checks. Reuse `saved_document_reopens_and_dirty_close_write_conflict_keeps_the_draft` as relevant.

**Acceptance / commit:** the required prompt distinction and persistence ordering work with no draft loss. Commit `fix(calculation-pad): [M4-A] persist remembered scratch before closing`.

### M5-A — Task E: drag the Diagram Viewer from preview content

**Objective / files:** add window movement through `src/gui/diagram_studio.rs` viewer input/effect ownership.

**Implementation:** collect primary drag intent from non-control preview space in native Mermaid graph, generic rendered Mermaid, and Visual preview modes, including empty/error states. Carry it through `PendingViewerEffects` and current accepted-pass, host and viewer-generation checks. Send `ViewportCommand::StartDrag` only to the native `viewer_viewport_id()` when that viewer is still the owner. Handle docked/embedded presentation through its egui window movement instead of dragging root. Exclude toolbar/minimap controls and all editor canvases.

**Behavior / edge cases / invariants:** drag moves the correct window, not nodes or the camera. Keep middle-pan, wheel/zoom buttons, Fit/100%, minimap and close/dock transitions working. Stale or discarded effects must not move a replacement viewer or launcher.

**Verification:** use existing viewer interaction/accepted-effect seams for target/ownership behavior; perform one focused native preview drag in each materially different render path when supported, and exercise a toolbar/minimap action plus middle-pan. No pixel-precision harness.

**Acceptance / commit:** primary preview drag moves only the intended viewer and preserves its existing interactions. Commit `fix(diagram): [M5-A] drag viewer window from preview content`.

### M6-A — Task F: put pending-change decisions in top host chrome

**Objective / files:** make Diagram Studio Save / Discard / Cancel visible independently of document scrolling in `src/gui/diagram_studio.rs`.

**Implementation:** render one shared decision banner near the host presentation button before either body ScrollArea. Remove the old Mermaid/Visual in-scroll copies. Continue generating `DocumentIntent::ResolvePending(TransitionDecision::{Save, Discard, Cancel})` with the existing transition description and accepted-action checks. Preserve `decision_save`, `decision_discard` and `decision_cancel` test hooks where useful.

**Behavior / edge cases / invariants:** dirty close/open/template actions show reachable controls even when scrolled to the bottom. Cancel preserves state; Discard performs only the pending transition; failed/conflicting Save leaves the decision available. Wrap within a small window and large text setting without covering host controls.

**Verification:** use existing decision behavior checks and a focused layout/interaction check for both document modes and native/docked host paths. Do not create an exhaustive screenshot matrix.

**Acceptance / commit:** one visible, reachable top decision panel in each mode with unchanged transition semantics. Commit `fix(diagram): [M6-A] keep pending changes controls at the top`.

### M7-A — Task G: compact status, sortable/filterable rows and reveal

**Objective / ownership:** improve presentation over the current Folder Size result/progress model without changing measured data.

**Files:** `src/gui/folder_size_dialog.rs`, the related accepted-effect dispatch in `src/gui/render.rs`, and existing `src/file_search/actions.rs` only if a narrowly required helper adjustment is necessary.

**Implementation:**

1. Keep a short status/subtotal/count summary; put verbose details inside a default-collapsed disclosure containing a bounded ScrollArea. Retain the model's bounded issue list rather than silently presenting only the first eight with no indication. The scanner's own truncation remains explicit. Reserve usable height for the data grid and keep fatal/action errors visible.
2. Add typed sort-column/direction and kind-filter state to the dialog. Make all displayed headers toggle direction and show the active ordering. Compare numeric size/share numerically; use deterministic ties and put unknown measurements consistently after known ones. Keep default size-descending behavior. Add a readable Type indicator and All / Files / Folders controls.
3. Derive visible ordered original indices for the current directory, cached on projection or presentation-state changes. Keep `show_rows` virtualization and stable generation/index identity. Do not sort/rebuild the entire scan each frame or recalculate subtotals from filtered rows. Keep Up/drill navigation available when filters hide all children; show a clear no-match state.
4. Add the row context menu and an accepted Reveal action. Right-click selects that row even if another row was previously selected. Resolve the row by generation/index after retained-pass checks, use `reveal_path` for selection in Explorer, and feed failure back into the dialog. Handle a missing path, stale scan, hidden selected row, root path and close/refresh races; never substitute another row after sorting. Do not invoke native shell actions during speculative rendering.

**Behavior / edge cases / invariants:** file/folder/omitted kinds remain truthful; sorting/filtering does not mutate scans or lose current-directory context; controls work while progress updates arrive and across dock/pop-out. Reveal and existing Open remain distinct.

**Verification:** focused comparator/filter tests for directions, ties and unknown values; one real row interaction covering sort/filter then right-click the intended target; stale/discarded action rejection; bounded details layout with several long issues. Extend `folder_size_dialog_projection_sorts_drills_and_rejects_stale_generation_selection` and relevant current pass-arbitration tests. Use a small disposable filesystem fixture for the native reveal smoke.

**Acceptance / commit:** all displayed columns sort both ways, kinds/filters are clear, details remain bounded, and reveal targets the correct entry. Commit `feat(folder-size): [M7-A] improve result controls and explorer reveal`.

### M7-B — Task G: increase scan capacity with explicit resource accounting

**Objective / files:** increase practical scan headroom in `src/folder_size/model.rs` and `src/folder_size/scan.rs` while preserving the shared scanner's validation in `src/file_operations.rs`.

**Implementation:** apply the larger directory and aggregate path targets from the Task G requirements and update both accumulator and traversal limits consistently. Replace the stale 3/32 discovery partition with the reviewed budget; update the accounting comment to match real simultaneous retained copies, progress/issue paths and root bookkeeping. Audit retained directory-handle lifetime/cancellation implications of the larger directory cap. Do not remove bounds, clone whole scans on every frame, or raise unrelated scanner defaults.

**Behavior / edge cases / invariants:** a representative shallow tree above 2,048 directories can complete when within the new limits. A path-heavy tree is no longer stopped by the old 3 MiB cap alone. Inputs that exceed any remaining bound still produce a truthful Partial result retaining verified subtotals. Inaccessible/cloud/reparse entries, cycles and changed roots remain safely handled.

**Verification:** prefer the existing fake metadata backend and small injected bounds to test exact limit behavior; include one representative synthetic tree above the old threshold with ordinary path lengths. Cover path-budget exhaustion and cancellation without scanning a user's entire drive. Reuse directly relevant `folder_size::scan` / model tests; do not rerun unrelated rename or file-operation suites unless shared behavior actually changes.

**Acceptance / commit:** the increased directory capacity is usable, the old discovery cap has coherent headroom, resource limits remain enforced and final values are documented. Commit `perf(folder-size): [M7-B] raise bounded directory and path capacity`.

### M8-A — Task H: correct Explorer Tabs baseline identity failure

**Objective / ownership:** restore saved-list opening by fixing the failing native observation boundary, preserving the current verified operation driver.

**Files:** `src/explorer_tabs/windows.rs`, `src/explorer_tabs/identity.rs`, and only directly affected seams in `backend.rs`, `native_uia.rs` or `src/gui/explorer_tabs_dialog.rs`.

**Implementation:**

1. Trace the reported preflight failure through `WindowsShellObserver::observe`. Add bounded diagnostic context sufficient to distinguish top-level enumeration, HWND/root/PID/TID resolution, Shell-row membership/process lookup, browser/view-root matching and recheck races. Preserve the immediate native error only where the API provides one; do not manufacture a Win32 error from stale last-error state.
2. Reproduce with a small saved list on a supported Windows session, with unrelated windows present and with an existing Explorer window. Identify the first failed responsibility before choosing the correction. The observed message is not proof of a COM activation defect; that code already has a separate capability path.
3. Fix that boundary using existing wrappers and worker ownership. A proven transient race may justify a small bounded fresh observation under the existing cancellation/deadline. A persistent mismatch needs its actual mapping/identity correction, not repeated sleeps. If separating complete handle inventory from strong Explorer identity is necessary, preserve exclusion of every pre-existing handle and target revalidation.
4. Keep preflight before window creation, strong action authority, exact folder/tab verification and operation ordering. Preserve partial completion/remaining folders on failure. Improve the user-facing diagnostic without declaring success merely because a request was dispatched.

**Behavior / edge cases / invariants:** supported saved lists reach the expected ordered tabs in the intended window. Zero/existing Explorer windows, a window disappearing during observation, stale/reused identities and genuine unsupported capability have bounded, truthful outcomes. Do not launch one Explorer window per folder, blindly issue keyboard input, disable baseline checks or skip unknown windows while claiming completeness.

**Verification:** create a deterministic regression at the actual failed seam; select directly affected `explorer_tabs::identity`, `backend`, operation and saved-list GUI tests. On Windows, open a small disposable saved list, inspect ordered tabs and confirm existing unrelated windows were not retargeted; exercise cancellation/failure where changed. If native reproduction is unavailable, document the unverified behavior and do not claim H functionally accepted solely on source review.

**Acceptance / commit:** the cause is identified and corrected, the requested saved-list workflow is demonstrated where the environment supports it, and target-safety regressions are covered. A diagnostic-only change does not satisfy H. Commit `fix(explorer-tabs): [M8-A] correct shell baseline identity observation`.

### M9-A — Final focused review and exact-tip handoff

**Objective / ownership:** close the whole A–H Goal with factual evidence. The parent coordinates a final independent review of the completed changes; the reviewer may focus on D–H and later corrections, incorporating the recorded A–C review rather than repeating unchanged work.

**Tasks:** inspect the cumulative diff, resolve feature-related findings with small follow-up commits, run only invalidated/missing targeted checks, perform one appropriate final first-party Clippy pass and classify unrelated pre-existing findings. Verify A–H status, the interim report/publication state and Git ancestry. Update the authoritative status and PR description with completion or precise limitations. Do not merge the PR automatically.

**Edge cases / constraints:** retain unverified native/build checks as unverified; retain unresolved publication as pending. Do not rerun the release build solely because final review began. If later changes actually invalidate a required release/feature check, perform the smallest appropriate recheck and state why. Do not absorb unrelated Clippy/vendor cleanup.

**Acceptance / commit:** sections J and K are satisfied, or the final disposition explicitly identifies genuine blocked items. Commit the final ledger/report update with `docs: [M9-A] record launcher utility fixes completion`. Then capture the exact final `HEAD` for the final report and next-Goal handoff. A tracked ledger may record the final report-commit lookup method; the final response/external ledger must include the actual `final_tip_sha` without a self-referential commit loop.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

M0-A is mandatory and distinct from the predecessor's final commit. The planned boundaries are M1-A, M2-A, M3-A, M3-B, M3-R, M4-A, M5-A, M6-A, M7-A, M7-B, M8-A and M9-A. Keep coherent implementation and meaningful tests together. A substantial correction may receive a descriptive follow-up checkpoint suffix; do not rewrite existing commits.

Before each commit inspect branch, `HEAD`, staged paths and diff. Stage only task-owned changes. Attempt one ordinary push after each commit. Record pending pushes, continue safely, and never change ancestry merely to make pushing succeed. The interim PR/review is not a merge boundary or a reason to create a new branch. A subsequent separately authorized Goal starts its own unique child at this Goal's `final_tip_sha`.

## H. Verification and Testing Strategy

Use the narrowest useful evidence for each changed behavior. Run one Cargo/build/test job at a time on the single machine. Use the repository's cached `fast-dev` profile for focused tests where appropriate. Confirm test filters match actual current tests; do not invent flags or treat an empty selection as a pass.

The command shape is:

```bash
cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(<actual-test-or-module-filter>)'
```

Replace the selector with real affected tests; the placeholder is not a runnable acceptance command. Relevant selection families include `studio_usability_query_focus_`, `notes_query_mouse_`, `launcher_show_focus_`, `calculation_pad::session::tests::`, `gui::calculation_pad_dialog::tests::`, `folder_size::`, `gui::folder_size_dialog::tests::` and `explorer_tabs::`. Select specific cases whenever the family would broaden scope unnecessarily. Keep focused pure-test execution near a 100-second upper bound excluding compilation, using existing process/timeout tooling; split slow selections rather than kill legitimate compilation or weaken tests. Stateful native interactions run serially.

For every application fixture/profile, set Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` before constructing the application, as required by `AGENTS.md`. Preserve production defaults and error notifications.

Build **`cargo build --release` at the end of C**, then use the resulting Windows executable for the relevant release smoke where available. Reuse builds/evidence; do not require repeated expensive production builds, the full repository suite, formal benchmarks, performance thresholds or distribution qualification. Later UI work may use the existing fast-dev build. Run a final appropriate first-party Clippy pass; exclude unrelated vendored cleanup.

For visible behavior, Codex should launch/interact with the application and capture concise screenshots or a short temporal capture when available. Use a few focused flows: A/B launcher interaction, D close/reopen, E/F viewer/prompt, G result controls, H a small saved list. Do not make the user run intermediate commands or provide screenshots. Never substitute source review or a synthetic event test for claimed native proof.

## I. Risk Management and Bounded Validation

| Risk | Mitigation / boundary |
| --- | --- |
| Focus fix bypasses a real owner or steals focus later | Reuse host layers, retained query responses and viewport/session identity; exercise a genuine modal and an editor-to-query round trip. |
| Thin LTO or profile inheritance changes iteration behavior | Inspect effective fields; explicitly preserve iteration stripping; use the ordinary production build and state tradeoffs without performance promises. |
| Idle optimization loses work or hangs shutdown | Treat readiness as a hint and preserve executor validation and a reliable shutdown wakeup. |
| Silent close loses Calculation Pad text | Persist successfully before closing; failure keeps the draft. Do not reuse Discard. |
| Sort/filter causes actions on another row | Maintain generation plus original index through accepted dispatch; never use the visible position as identity. |
| Larger scans exceed bounded storage or retain too many handles | Review path partition and lifetimes, retain caps, use small/fake boundary tests and cancellation checks. |
| Explorer fix weakens new-window/target evidence | Keep a complete exclusion baseline and fresh strong target identity; fix the actual observation boundary. |
| GitHub publication blocks the rest of the Goal | Commit the review locally, record publication pending, continue D–H and reconcile later without a retry loop. |

Use one focused verification pass per checkpoint, with up to two targeted correction/recheck attempts for a concrete failure before reassessing the blocker. This is not permission to abandon routine implementation or to leave a known fix unfinished. A new specific cause can justify a further bounded step; avoid repeating identical unsuccessful attempts.

An unavailable Windows desktop, screenshot backend, compiler target or GitHub credential is an environment limitation. Record the exact unrun/failed check, its implication and the smallest next action. Continue all remaining safe implementation and independent checks, including D–H after the interim review, without asking the user to perform intermediate acceptance. Keep affected acceptance items open; the final status must not claim complete functional verification where evidence is missing.

Actual unsafe repository state, unresolved ancestry, potential draft loss or a defect that prevents safe dependent work is a genuine blocker. Preserve work and write the concise tracked stop report required by `AGENTS.md` under the authoritative execution directory, with actual commands/results, branch/HEAD/push state, attempts and smallest next step. Unrelated historical test/lint failures and optional enhancements are recorded separately and do not stop this Goal.

## J. Definition of Done

- [ ] M0-A used the correct exact parent, a fresh unique task branch and its own direct-child baseline; pushes and user-change preservation were recorded.
- [ ] A, B and C were implemented before D–H, with their required focused checks and truthful build/native status.
- [ ] The independent A–C peer review is committed and published on GitHub with its exact reviewed SHA and a usable report/PR-comment URL. A pending publication is reported as an outstanding delivery limitation.
- [ ] The parent continued from M3-R directly through D, E, F, G and H without waiting for user/PR approval or creating another branch.
- [ ] All requested functional behavior A–H is implemented and its relevant acceptance criteria pass; no known feature-blocking defect remains.
- [ ] Relevant first-party lint/review findings are fixed or accurately classified; unrelated warnings and vendor code were not absorbed into scope.
- [ ] Verification records distinguish tests/builds actually run, native checks actually observed, and anything environment-blocked. Limited verification is never reported as complete acceptance.
- [ ] All intended changes are committed, actual push/checkout state is recorded, and the final response contains `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs and exact `final_tip_sha`.
- [ ] The following Goal is instructed to branch from this exact `final_tip_sha`; no worktrees, auxiliary clones, independent checkpoint branches or intermediate merges were used.

Full completion requires the requested behavior and delivery, not merely reaching task C or writing all source changes. If a genuine external limitation remains after all feasible work, use a factual LIMITED/BLOCKED disposition identifying exactly what remains.

## K. Final Delivery Report

Give the user a concise, self-contained report with:

1. A–H status and completed checkpoint list, including the interim A–C review and the subsequent D–H continuation.
2. User-visible changes, important decisions and the relevant source files.
3. Actual test/build commands and results, release settings, expected idle benefit without invented metrics, and native evidence where available.
4. The A–C reviewed code SHA, committed report link, GitHub PR/review-comment URL, and any publication limitation.
5. `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, checkpoint commits, exact `final_tip_sha`, actual push status and checkout cleanliness as of the report.
6. The explicit handoff: **the next Goal must create its new child branch from this exact `final_tip_sha`, not from `master`/`main`; no worktrees or intermediate branch merges were used.**
7. Any remaining functional/verification limitation, its evidence and the smallest next action. Offer one final compact user click-check list after all feasible implementation, not intermediate manual tasks.

Do not claim a GitHub publication, native interaction, test or build passed unless it actually did.
