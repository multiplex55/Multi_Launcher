# Codex Kickstart — Studio and Utility Polish

Execute **CODEX_GOAL_STUDIO_AND_UTILITY_POLISH.md** autonomously through its final handoff. Read the repository's AGENTS.md, applicable local instructions, this Kickstart, and the entire Goal before beginning. The Goal is the feature contract; current repository instructions govern engineering and configured agent orchestration.

## 1. Mandatory First Execution — M0-A Chained Git Bootstrap

Before changing application code, running implementation builds/tests, or starting another milestone, complete this sequence in the **single existing repository checkout**.

1. Inspect git status -sb, current branch and HEAD, remotes/upstream, staged/unstaged/untracked work, and the prior Goal/category's recorded final handoff. These checks are read-only.
2. Determine the exact immediately preceding completed development Goal/category tip. Record parent_branch and parent_tip_sha. **Every subsequent Goal starts directly from the preceding Goal's final_tip_sha, not master/main or an older common base.** Only the first Goal in a genuinely new chain defaults to an up-to-date primary integration branch, unless the user explicitly supplies another base.
3. At planning time, the predecessor candidate is **codex/launcher-utility-fixes at d691888584d531d9167f035dd48e47d15dced483**. Its source work is terminal LIMITED because native checks remain open. Master **838f8a42ff4c938a314065d5d3b79f3ac5425195** later merged it. Do not use that merge as permission to abandon the exact predecessor-tip rule, and do not let older STATUS timestamps override current refs. Reconcile the current ledger and refs; if a newer completed category has since handed off, use that verified final tip.
4. Fetch safely when the necessary predecessor is only remote and verify its exact SHA. A local predecessor whose push failed is still the real predecessor. If execution moved to another machine without that commit, block only this prerequisite until the ancestor is available; do not substitute stale remote state.
5. Choose a **fresh, uniquely named, idiomatic lowercase branch**, provisionally **feat/studio-and-utility-polish**, using a suffix if that name exists. The Goal and Kickstart share this one branch. Do not omit the child branch merely because the checkout is already on a task branch.
6. Protect existing user work. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite, or stage unrelated changes to make the transition possible. If safely selecting the required parent is impossible, report the exact blocker instead of choosing another parent. A documented safe current-HEAD base is allowed only for the first task of a genuinely new chain with no predecessor.
7. Create and switch to the child directly at the verified parent SHA, equivalent to git switch -c feat/studio-and-utility-polish <parent_tip_sha>. Verify active branch and initial HEAD. No implementation on the predecessor or on master/main.
8. Create the new branch's own **M0-A baseline commit**. Prefer the Goal's task-owned execution ledger; an intentional empty baseline commit is acceptable. Stage only intended task material. Record baseline_sha separately from parent_tip_sha and verify that git rev-parse <baseline_sha>^ equals parent_tip_sha. Do not relabel the predecessor's existing tip as this Goal's baseline.
9. Attempt one ordinary non-force upstream push, using the verified configured remote/upstream or git push -u origin HEAD. If unavailable or rejected, record the actual reason, keep the local branch/commit, and continue safe work. No retry loop, history rewrite, or security bypass to force a push.
10. Keep every later checkpoint on this task branch. **No per-agent/per-checkpoint branches and no intermediate merge, rebase, squash, or cherry-pick merely to progress between Goals.** At completion, record the actual final_tip_sha as the required parent of the next fresh child branch. The last tip of the chain contains the cumulative work; final integration is a separate approved boundary.

**No Git worktrees or alternate implementation directories.** Do not run git worktree add, move, or prune. Do not create auxiliary checkouts, secondary clones, or agent-specific clones. All source writes, builds/tests, Git operations, and branch transitions use the existing checkout sequentially.

M0-A is complete only when the fresh active branch, verified exact parent, direct-child baseline, initial push outcome, and preservation of user changes are recorded. Resolve an unsafe/conflicting ancestry prerequisite before application work; do not silently guess.

## 2. Revalidate the Current Source and Establish Ownership

After M0-A, inspect the relevant current files and tests identified in Goal section B. The execution checkout is authoritative. Adapt moved paths while preserving the approved behaviors and scope.

Key owners:

- **Folder Size:** src/gui/folder_size_dialog.rs, src/gui/render.rs, src/file_search/actions.rs.
- **Calc Pad and Programmer Inspector:** src/gui/calculation_pad_dialog.rs, src/gui/programmer_inspector_dialog.rs, and existing calculation_pad comparison/session/model APIs.
- **Diagram shell and navigation:** src/gui/diagram_studio.rs and its existing camera, menu, document-intent, and accepted-effect boundaries.
- **Mermaid colors:** src/diagram/mermaid.rs, model.rs, session.rs, codec.rs, scene.rs, rendering.rs, export.rs, and templates.rs; a narrow patch of the existing pinned renderer only where the Goal demonstrates a color-support gap.

Do not repeat a repository-wide architecture investigation. Confirm the actual owner, adjacent callers, and relevant tests, then implement.

Use the Goal's proposed task ledger under docs/plans/execution/standalone/studio-and-utility-polish/ unless an active authoritative queue ledger already governs execution. Keep one source of execution status.

## 3. Use Configured Agents and One Write/Build Lane

Use the repository's current planner, implementer, and reviewer roles where available. Read their configuration; do not replace it with older model preferences or invent unavailable roles/tools.

- **Planner:** Confirm dependencies, ownership, current-source differences, acceptance criteria, and the next bounded checkpoint packet.
- **Implementer:** Own one coherent implementation checkpoint at a time in the shared checkout.
- **Reviewer:** Independently inspect the cumulative changes and directly relevant surrounding code for concrete defects, incomplete requirements, ownership problems, and missing meaningful verification.
- **Parent/orchestrator:** Own task status, sequential delegation, the single build/test lane, checkpoint commits, push attempts, blockers, and final reporting.

Every implementation handoff includes objective, existing owner, exact scope, required behavior, edge cases, invariants, non-goals, tests, verification, and done criteria. Do not make the implementer reconstruct the plan from a broad narrative.

Parallelize useful **read-only** investigation/review only. Never run competing source writers or Cargo jobs. All agents use the same existing checkout and task branch.

## 4. Execute the Whole Goal in an Efficient Order

Default sequence after M0-A:

1. **M1-A:** Folder Size cross-drive reveal and toolbar label.
2. **M2-A/B/C:** Comparison input authority and Pad actions; grouped/compact bits; complete Copy All report.
3. **M3-A/B:** Diagram menus/editor/header; larger Visual canvas and navigation.
4. **M4-A/B/C:** Native/source styling semantics; actual generic renderer palettes; color controls/templates/help.
5. **M5-A:** Consolidated verification, independent review, remediation, and final handoff.

The user explicitly authorizes rearranging requested features for implementation speed or architectural sense. The independent M1/M2/M3 groups can move relative to one another when the reason is recorded. Keep stable checkpoint labels, preserve M0-A as the first step, keep semantic fixes before their dependent UI, and finish M4-A/B before presenting M4-C's color controls.

**Do not stop after a subset of tasks, an interim review, or a checkpoint push.** Continue the remaining approved work without asking whether to proceed. Do not import a previous Goal's A–C publication gate, milestone order, or native acceptance campaign into this Goal.

If one native check is unavailable, record it and continue safe independent implementation. If a concrete implementation blocker affects only one group, preserve that work and complete the other independent groups before the final bounded report.

## 5. Essential Implementation Decisions

Follow the detailed requirements and checkpoint packets in the Goal. In particular:

- **Comparison already exists.** Reuse ComparisonContext and ComparisonSlot. Fix the discrepancy between visible text and hidden selected references; do not add another evaluator. Typed rN/names must work with no selected integer. Optional selected-result input remains stable by LineId but visibly identifies its actual current row; editing returns it to typed-expression mode.
- **Copy All uses real source.** Produce the three labeled sections specified in the Goal from matching accepted source/evaluation. Preserve original physical rows, assignments, Unicode, errors, and genuine rN text. Keep per-result Copy unchanged.
- **Bits stay exact.** Change layout, not arithmetic. Keep 8/16/32/64/128-bit behavior, signedness, checked casts, explicit reinterpretation, and private insertion semantics.
- **Visual reset means fit to content.** Reuse the full current Fit behavior, including restoring viewer resize-follow after reset. Navigation does not edit the document. Only the Visual Document viewer gains primary-button pan; Studio object dragging and other viewer contracts remain intact.
- **Tab and menus use existing mechanisms.** Enable TextEdit focus locking, reuse document intents/menu focus scope, and keep the source editor stationary across preview-state changes.
- **Colors must be persistent and real.** Support the current 13 templates/11 families using a typed bounded source representation. Native flowchart style changes must survive later structural edits, source formatting, and the existing Mermaid-to-Visual copy where colors are representable. Mindmap requires explicit palette wiring; Theme alone is insufficient. Qualify actual color consumption per family and identify any renderer-specific palette keys honestly in Help.
- **Renderer patch is narrow.** Keep mermaid-rs-renderer pinned at 0.3.1. Follow the Goal's actual family color mapping and minimal Timeline event-card palette patch; retain unstyled defaults and geometry. Preserve Gantt status colors and Journey's semantic score scale. Do not replace the renderer, add a browser, implement unrestricted CSS/configuration, or upgrade dependencies broadly.
- **Folder Size needs diagnosis at the actual boundary.** The working toolbar and failing context menu use different Explorer paths. Preserve the absolute target and retained ownership checks; do not assert drive-letter loss without evidence.

No unrelated refactors, new workspaces, release optimization, global focus redesign, or Explorer Tabs recovery is included.

## 6. Verify Proportionately and Truthfully

Use Goal section H and the repository's current testing instructions.

1. Run narrow package/module/test filters with Cargo Nextest and --no-fail-fast. Use the existing fast-dev Cargo profile. Resolve test names from current source; existing names in the Goal are anchors, not a giant mandatory suite.
2. Keep one Cargo process at a time. Separate compilation from test execution. Bound each focused execution batch to **100 seconds** using the existing owned-process runner or equivalent supported facility; preserve cleanup/output-drain evidence on timeout.
3. Disable Paste Sequence next_hotkey and cancel_hotkey in test profiles before app construction. Reuse the established test-only stack setting if the relevant large fixtures still require it.
4. Add/adjust tests for concrete behavior: comparison input transitions, same-frame source/result copy consistency, bit index mapping, camera versus window movement, Tab focus and stable geometry, style persistence/export, actual family colors, and stale render rejection.
5. A label/menu layout change alone does not require a new test framework. Do not mirror implementation details or preserve obsolete geometry assertions.
6. Build and launch the actual application for one consolidated smoke pass when supported. Use a synthetic profile and disposable files/folders; capture useful evidence. Verify affected docked/popped-out flows and the cross-drive Explorer result.
7. Do not call a queued/spawned Explorer action, synthetic state assertion, or successful render equivalent to an observed native target or visibly applied palette.
8. If Windows, another volume, input/capture, or a required environment is unavailable, mark the exact check unverified. Do not invent a drive, disable safeguards, ask the user to do intermediate testing, or retry an unchanged environment indefinitely.
9. Perform appropriate formatting/diff checks and one scoped first-party Clippy pass near completion. Fix introduced issues; record unrelated pre-existing findings and avoid unrelated vendored cleanup.

No automatic full test suite, repeated release builds, benchmark campaign, broad native harness, or historical acceptance matrix is required. Broaden only for a concrete risk exposed by the changed code.

## 7. Commit and Continue

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** Section F of the Goal defines those boundaries.

Before each commit, inspect current branch/HEAD, staged paths, and the actual diff. Verify only what the checkpoint needs. Commit coherent verified increments on this same branch, using the M#-X label and a descriptive subject. Do not stage unrelated work, squash/rewrite prior checkpoints, or create new branches for milestones.

After each checkpoint, attempt one ordinary best-effort push when supported. Preserve successful local commits on failure and continue. Record actual push status without claiming GitHub visibility for unpushed work.

When user input arrives during execution, treat it as steering unless it clearly cancels/replaces the Goal. Keep the remaining approved work active.

## 8. Bounded Blockers and Final Independent Review

Use the Goal's small corrective budget for repeated external/native failures. Normal compiler errors are implementation feedback; fix the cause and rerun the smallest useful check.

At final review:

- Check every requirement ID against the actual diff and evidence.
- Confirm comparison text/binding truthfulness and preserved exact numeric behavior.
- Confirm Folder Size target ownership and native cross-drive evidence or its explicit absence.
- Confirm Visual input changes affect the correct context and cannot also move the window.
- Confirm colors survive source/model edits, history, save/reopen, and export, with correct cache/publication identity.
- Confirm menus/help preserve dirty-document and retained-host behavior.
- Resolve concrete feature-related findings in small remediation commits and rerun only affected verification.

Do not convert review into another broad design project. If an actual external blocker remains, follow AGENTS.md's tracked stalled-goal report policy in the task-owned docs path when safe. Record the exact failure, evidence, attempted remedies, preserved work, and smallest next action. A documentation-only stop commit is not an accepted implementation checkpoint.

## 9. Finish at the Correct Boundary and Hand Off the Exact Tip

Stop when the approved Goal is implemented and its required bounded verification/review are complete, or after safe independent work is exhausted and a genuine remaining blocker is documented. Do not continue into optional features.

Report **Complete** only when the required acceptance evidence supports it. If implementation is finished but a required native check is unavailable, use **LIMITED** and identify precisely what remains unverified. Never quietly waive a requirement to obtain a green status.

After the actual final commit, read and report:

- parent_branch and parent_tip_sha.
- Task branch and baseline_sha.
- Significant checkpoint subjects/SHAs.
- Exact final_tip_sha.
- Actual push outcome and clean/dirty checkout state, with unrelated user changes identified.

Verify the baseline's direct parent and the task's linear ancestry. Confirm that no Git worktrees, auxiliary checkouts/clones, agent/checkpoint branches, or intermediate branch merges were used.

The final response must explicitly say: **The next Goal must create a fresh child branch directly from this final_tip_sha, not from master/main.**

Put the exact final SHA in the final report and any authoritative external handoff ledger after the last commit. Do not generate endless self-referential commits to embed a commit's own hash in its tracked contents.

Deliver the Goal's final report: completed checkpoints, important decisions/files, actual verification results/evidence, remaining issues, Git ancestry/push state, and one simple manual checklist for the user. The user performs their review after your implementation work finishes; no intermediate approval or manual-testing stop is required.
