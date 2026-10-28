Codex Kickstart — Resume Goal 04/M2-A and Continue the Existing 17-Goal Multi Launcher Queue
Filename: CODEX_KICKSTART_RESUME_GOAL04_M2A_20261026.md
Repository: G:\Repos\rust\Multi_Launcher / multiplex55/Multi_Launcher
Existing queue ID: multi-launcher-queue-20261025-eedc19d2
Execution mode: Existing parent orchestrator; configured Planner, sole source-writing Implementer, and Reviewer; strict cumulative goal order.
Purpose: Authorize a single, narrow, finite renewal of the remaining Goal 04/M2-A live compiler-origin/Neovim startup-contention acceptance boundary. After verified acceptance, finish Goal 04 and automatically resume Goals 05–17 under the original queue contract.
Important: This supersedes the recovery-point and repair-budget wording of the earlier CODEX_KICKSTART_RESUME_MULTI_GOAL_QUEUE_20261025.md. It does not supersede accepted earlier work or authorize a new queue.

1. Execute the existing queue; do not create a new plan
You are the parent Codex orchestrator for the existing queue. Read this kickstart completely and perform it now. Do not respond with only an analysis, replacement plan, or request for routine approval. Use existing QUEUE_STATE.md as the single operational source of truth, and reconcile it against Git, preserved receipts, and process state before any mutation.

Applicable authority, within platform/tool permissions:

This document only for the expressly renewed M2-A recovery boundary and its safe resumption.
docs/plans/CODEX_KICKSTART_MULTI_GOAL_QUEUE.md for the existing 17-pair order, cumulative branches, M0-A, agent roles, acceptance/review gates, recorded state, resumption, and goal handoffs.
docs/plans/CODEX_GOAL_04_LINK_EDITOR.md plus docs/plans/CODEX_KICKSTART_04_LINK_EDITOR.md for Goal 04's actual requirements, bounded verification, implementation scope, and M2-A/M2-B checkpoints.
AGENTS.md, applicable nested instructions, .codex/config.toml, and configured .codex/agents/{planner,implementer,reviewer}.toml.
The existing external queue ledger, exact Goal 04 BLOCKED_RESUME.md, actual raw results and immutable inputs, and the live repository/worktree state for the most recent facts.
The original individual Goal/Kickstart pairs when their subsequent Goal is entered.
Only explicit queue override here: Section 4 grants two additional targeted correction rounds in total for the already exhausted Goal 04/M2-A compiler-origin diagnostic acceptance boundary, inclusive of its observed Rustaceanvim startup DAP contention. All other repair budgets, deadlines, acceptance requirements, and historical fail statuses remain unchanged. A correction round is a bounded patch plus targeted checks and, if indicated, at most one bounded new live validation. No automatic resets by renaming the fixture, controller, or failure type.

Never rewrite a failed receipt as passed. Never describe semantic-only E0308, a cargo check, a Lua self-check, or a headless/version probe as accepted live compiler-origin diagnostic evidence.

2. Latest reported verified anchor — verify actual state; do not reset to it
The most recent consolidated stopping report records:

Property	Last recorded state
START	bbc2ca1ef247c53eeae9f2ce7ba69f381483318d
Queue and checkout	multi-launcher-queue-20261025-eedc19d2; G:\Repos\rust\Multi_Launcher
Goals 01–03	Complete, frozen, pushed; do not reopen
FINAL_01	dd79543aa58e97036014d4c78ecc990ce8f62900
FINAL_02	2b1e6a07d091ecdfdf5f85f660f57b53f5f2ab77
FINAL_03 = BASE_04	6a3d612fcceafe1423341f835cf1f5f2fa317ff5
Active branch	codex/windows-link-editor-queue-20261025
Goal 04 M0-A	48946184cb395d8bc501a251dc4ea77b4facf603
Accepted/pushed M1-A	55d81350718adea3a8a4ca72f56e7695e836b7b9
Accepted/pushed M1-B	ceff1f17fb09545d8b9a6010aae9eca7892a7ea7
Local HEAD and upstream	ceff1f17fb09545d8b9a6010aae9eca7892a7ea7 in the stopping report
Goal 04 M2-A	Partially implemented and partly verified; mandatory live compiler diagnostic/clearance/isolation-revert acceptance incomplete
Goal 04 M2-B	Dependent; not accepted
FINAL_04	Does not exist in the stopping report
Goals 05–17	Not started; no later cumulative branches or final anchors established
Critical correction to the earlier recovery kickstart: M1-B is now accepted and pushed. Do not rerun its completed work, issue another M1-B correction packet, repeat paired linker benchmarking, or manufacture a duplicate checkpoint. Its reported paired whole-command changes were application edit 41.135731 → 36.693609 s (10.7986947% lower) and selected root edit 54.345498 → 47.465428 s (12.6598711% lower), with scoped native/resource/PDB/selected-case evidence. These are single paired observations, not isolated linker timing or a statistical claim. Preserve historical limitations recorded in the ledger, including the unretained app caller exit and the aggregate Nextest parser's Complete=false; their accepted parent adjudications must not be silently rewritten.

These SHA values are historical anchors, not a request to switch/reset/rebase to an older commit. If the queue has advanced beyond them in verified local state, reconcile and follow the verified later state. If FINAL_04 already exists, validate its handoff and proceed without reopening Goal 04.

3. Mandatory safe preflight and exact recovery evidence
Before source edits, new tests/builds, or a new Neovim session:

Locate and read the existing external QUEUE_STATE.md beside the latest consolidated stop report, using the real path recorded in the queue ledger. Never create a competing ledger. Read the original master queue and Goal04 pair, current goal-04.md, and the current actual M2-A implementation/observer code.
Read in full g04-recovery-20261026/m2a-live-r09/BLOCKED_RESUME.md and parent-result-audit.json; open the exact execution/live-20261026T162848181Z-dca0aa866f384749ab6ade9e7f9f8847/controller-result.json. Locate via the ledger/evidence root rather than assuming those relative paths are under the source repository. Review r06/r08/r09 original execution inputs and receipts, accepted earlier Lua/observer/readiness/queue-drain tests, and the parent audits.
Verify the r09 final manifest e21653484267ffd4ad48dd779dc2c2dab313728f47d47846c1cd637da0353e3a and its relevant preserved snapshots/bindings. Inspect the preserved RA log (446 bytes, SHA-256 6996437c8db0fe751957919491d6d20025b9ab7d61168bc0c6e30bea4f1c3af4), recorded actual queue/start/callback transitions, and the exact final r09 observer/controller inputs. Do not treat a digest discrepancy as permission to overwrite evidence.
Check actual repository origin, default branch, current local branch/HEAD, configured upstream, ancestry, index, worktree diffs and concurrent writers. Fetch only by safe normal operations as permitted. Do not make a new branch or M0-A for Goal 04; the accepted M1-B commit must stay in the ancestry.
Preserve the four intentionally uncommitted Goal04 task files recorded after r09:
dev/README.md
dev/editor/rust_analyzer_profile.lua
dev/editor/test_rust_analyzer_profile.lua
docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/goal-04.md And preserve the existing empty untracked/unstaged nvim.log. Compare task-file identities with final r09/preserved-working-files.json and its immutable snapshots. Do not mistake earlier reports of seven pending files for the current four.
Read and verify the last accepted scoped evidence: Lua self-check PASS (0.596660s), four observer tests, one readiness test and one queue-drain predicate test, partial UI-attached effective/client/semantic evidence, and independent direct all-workspace/all-target cargo check (exit 0, 20.431543s). Preserve rather than blindly rerun those exact tests when the corresponding inputs remain unchanged.
Reconcile process owners by PID plus creation-time identity, not process name. The report states r06 359, r08 336, and r09 341 recorded identities retired; confirm no in-flight lease/owned process/writer before creating one. Respect output-drain, restoration and cleanup rules. Do not kill user's existing Neovim/Rust/Cargo processes.
Write a small resume-intent entry into the same external ledger: exact observed Git/files/process facts, the renewed two-round count, last confirmed baseline, precise next bounded diagnostic action, and provenance of decisions. This is an administrative ledger update, not a new checkpoint or branch.
If repository/ledger identity, preserved source bytes, concurrent writer, or required evidence cannot be safely reconciled, report the exact blocker and stop before unsafe modification. Do not discard, stash, reset, force checkout, clean, or rewrite history.

4. Explicit narrow renewed allowance: only the unfinished M2-A live gate
4.1 What actually failed
Treat the reported observations as separate facts:

r06: The strict compiler-origin diagnostic gate failed; a worker left the fixture, but the outer controller restored the original. Do not claim a compiler diagnostic from that attempt.
r08 (remedy 1): A didSave event was queued, an RA-owned Cargo check was observed, and same-file semantic E0308 from rust-analyzer appeared. No accepted rustc-origin fixture diagnostic was observed within 150s. Semantic E0308 is not a substitute.
r09 (remedy 2, final): The controller exited 1 after 209.2335899s; worker exited 2 after 205.828s. Before the fixture was written, actual startup DAP queues failed to drain in 150s. Both runnable responses/deferred timer returned, but reported 3 starts / 8 system callbacks, with 2 starts / 2 callbacks still active. The fixture was not reached. A core cargo check --no-run equivalent completed with exit 0 and noted package-cache/build-directory lock waits (29.71s); unfinished jobs had no completed result and must not be called failed or successful. The RA log records watch-path/long-loop warnings only.
On r09 stop, all source-byte/environment/process-restoration and output-drain audits succeeded. The actual problem is unmet editor acceptance, not known leaked or still-running owned processes.
Hypotheses, not established facts: Startup Rustaceanvim DAP autoload/configuration work and competing Cargo package-cache/build locks may contribute to the stalled startup queue. The report does not prove the private cache is the sole root cause. Investigate cause with actual installed plugin/source/configuration and event receipts; do not guess or simply add time.

4.2 Two-round scope and safety contract
This document authorizes at most two new focused correction rounds total, limited to the combined M2-A live compiler-origin diagnostic / startup-DAP admission contention acceptance boundary. Every round must have one recorded defect hypothesis, one scoped source/fixture correction when justified, minimal supporting static/unit checks, at most one new bounded actual editor validation, and a captured result. Record intent and stop conditions before each owned action. Preserve all r06/r08/r09 failures and accepted earlier checks unaltered.

Not authorized: unlimited rounds; resetting the count after a different symptom; rerunning Goal 01–03 or Goal04/M1-A/M1-B; changing real user's global Neovim or Cargo configuration; editing installed rustaceanvim private caches or source in place; weakening the requirement for compiler-origin diagnostics, real effective settings, workspace isolation or exact revert; extending existing scenario deadlines merely to outwait a quiet job; killing processes by name; silencing stderr; fabricating clearance, link speed, process exit, or editor acceptance.

Use the configured Planner to inspect the current installed Rustaceanvim version and initialization lifecycle, actual owned test-adapter code, effective public DAP/config settings, whether internal configuration is snapshotted/cached, and the RA/Cargo build/check command queues. The Planner supplies a single minimal implementer-ready packet. The Implementer is the sole source writer; the Reviewer or explicitly permitted parent fallback reviews stable diffs. Do not create multiple competing editor observer processes or competing Cargo builds in one target/cache scope.

Important known trap: Setting public dap.autoload_configurations=false after plugin initialization is not an acceptable fix by itself: installed internal Rustaceanvim configuration caches the value. A public vim.g restore that leaves effective cached private state changed is a regression. Prefer genuinely supported pre-initialization, process-local configuration in a disposable owned Neovim session, or another externally observable, reversible solution compatible with the actual plugin. Preserve the declared working light/full mode, user's normal global settings, and exact effective state after reverting. If a safe effective restore cannot be established, report blocked, rather than quietly mutating plugin internals, monkeypatching global installed files, or dropping mandatory acceptance.

4.3 Required correction investigation — before rerunning expensive live sessions
The correction packet must inspect and record:

Which actual owned async jobs, DAP startup requests, RA check requests, and Cargo metadata/build/check operations account for the 3 starts/8 callbacks and 2 starts/2 callbacks still active at the r09 timeout. Resolve how the test calculates "drained" and whether outstanding items represent actual executing owners, queued requests, or retained bookkeeping. Preserve honest unknown states.
Whether Rustaceanvim's actual supported early opt-in DAP configuration can avoid unrelated startup work for this isolated, disposable observation session without changing the user's live configuration or suppressing required compiler diagnostics. Verify timing of module initialization/cache creation and the conditions for exact reversion. Do not assume late reassignment to vim.g changes cached config.
Whether overlapping Cargo operations compete for package/build directory locks or rust-analyzer check slots, and whether the owned harness itself is introducing unnecessary contention. Prefer serialized relevant owners or a separately scoped supported test target/cache when clearly safe and equivalent. Do not clear caches, kill external Cargo, change global RUSTFLAGS, or substitute an irrelevant project.
What concrete evidence identifies a rustc/compiler-origin diagnostic from the actual rust-analyzer check pipeline (e.g., provenance within LSP publishDiagnostics, underlying captured compiler/check output, and matching source/range/message), as opposed to only rust-analyzer semantic analysis. Validate actual checkOnSave/runnable/config state at attach; don't trust a configuration file's intended values alone.
Whether the existing project-root profile, nested core workspace admission, foreign/nofile refusals, semantic navigation, effective full/light mode changes and exact revert remain valid. Do not broaden profile reach beyond the intended project or weaken trust boundaries just to make the observer pass.
A quick read-only or low-cost adapter-level preflight demonstrating corrected ownership/queue bookkeeping should precede another 150s live startup drain. Keep such preflights focused and count any corrective edits toward the two-round cap. Respect original limits; do not relabel timeouts or missing receipts.

4.4 Complete the original M2-A live acceptance, not a surrogate
After safely correcting the specific startup barrier, run one bounded, actually UI-attached Neovim + installed Rustaceanvim/rust-analyzer observation per correction round when justified. Capture proof of all applicable original M2-A gates, reusing still-valid unchanged earlier observations where permitted:

Effective client/server configuration under the explicit light and full modes, including real diagnostic/check behavior; a version or plain config dump alone is insufficient.
Project-root and declared nested table-core admission, plus meaningful non-project/foreign and nofile refusal; no security/trust bypass.
A real, deliberately controlled Rust source diagnostic from the compiler/check pipeline, positively identified as compiler-origin in the actual client/received observations. Never accept semantic-only E0308, a check-job start event, or cargo check success as proof of this requirement.
Correct same-file compiler error appearance and clearance after an actual controlled repair/save/check cycle, with attributable events/identities and a truthful time-bound receipt.
Meaningful semantic definition/navigation and isolation behavior, and an exact public and effective runtime revert (including relevant cached state or safe lifecycle boundaries) without leaving changed settings in the user's Neovim environment.
Outer raw-byte backup/restoration guard installed before the first source write, verified source hash/length after completion, no persistent project/global config leakage, and owned PID+creation-identity cleanup/output drain on all exits.
Use the existing Goal04 limits: 30 minutes per selected compile/link command, 5 minutes per editor scenario, 2 minutes post-build application smoke, and the original separately bounded test/cleanup ceilings. Maintain the existing bounded startup/DAP sub-phase rather than silently extending it. If expected jobs fail to drain, fail early and preserve evidence rather than stretching a total deadline or launching another concurrent session. A timeout is a failure/unknown evidence, not a success.

If the first renewed round succeeds all mandatory M2-A gates, do not consume the second. If the two renewed rounds both fail a mandatory criterion, record exactly what was proven and what remains missing, mark Blocked, and stop before M2-B or Goal05; preserve the partial source, failed receipts, immutable inputs and exact next action. Do not invent another budget extension or pivot to a different, weakened test.

4.5 M2-A acceptance, review, commit and push
Once all original mandatory M2-A evidence is genuinely satisfied (including compiler origin, clearance, isolation and exact revert):

Review the exact changed sources and the relevant prior accepted evidence. Use the configured Reviewer if available; if its dispatch is unavailable due to the recorded agent thread limit, follow the master's explicitly authorized parent fallback rather than bypassing requirements. Label parent fallback accurately as not independent; it cannot accept a missing mandatory gate.
Update the current tracked Goal04 execution record with the truthful M2-A inputs, results, scope/limitations and review disposition. Do not edit frozen Goals 01–03 or rewrite accepted M1-A/M1-B results.
Stage only task-owned accepted changes. Make one coherent [M2-A] checkpoint commit on existing codex/windows-link-editor-queue-20261025, attempt one normal non-force push, and record actual SHA/upstream result in QUEUE_STATE.md. Do not amend/squash previous commits, force-push, change branch, or invent FINAL_04 yet.
5. Then finish Goal 04/M2-B, final handoff and original queue
Immediately after verified and committed M2-A:

Execute the original Goal04/M2-B comparative link/editor responsiveness checkpoint, exactly as specified in Goal04 and its Kickstart. Reuse M1-B's accepted linker outcomes unless genuinely invalidated by changed effective inputs. Perform the actual required live editor comparisons with matching baseline and provenance, using the permitted finite repeat budget; no fictitious speed gain or no-gain claim. Preserve opt-in/full-debug/canonical-release escapes.
Complete focused verification, actual native/editor evidence, scope review and any finite corrections. Run applicable first-party Clippy last, after final tracked changes/tests/review and before source freeze. Do not clean up unrelated vendored warnings; do not run an automatic full Nextest suite. Record the exact staged tree/source identity verified and confirm the final commit tree matches it without changes.
Commit and attempt normal push for the accepted [M2-B] checkpoint. Perform the original master Section 10 completion/handoff gate. Only if all Goal04 mandatory checks pass: record immutable FINAL_04, freeze the Goal04 branch at its accepted final SHA, and update QUEUE_STATE.md with actual commit/push/review/evidence facts.
Automatically enter Goal05, create exactly one new branch and one M0-A from the verified FINAL_04 (never from master or Goal04's partial SHA), and process original Goal/Kickstart pairs 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17 sequentially. Before each new Goal, verify the predecessor's FINAL_NN ancestry and all required inherited capabilities. Specifically verify Goal10's Goal09 dependency and Goal17's Goals09/10/12 capabilities.
Follow the master queue's fixed scope, sole-writer policy, configured agents, focused native/acceptance checks, finite correction budgets, checkpoint commits/pushes, end-only Clippy, exact-tree checks, evidence ledger and final report for each later Goal. No routine mid-goal user approval or manual intermediate tests are required; user manual review occurs after completion or a genuine stop.
No new queue, no duplicate Goal04 branch/baseline, no skipping blocked Goals, no bypassing required predecessors, no workspaces feature, no merge into master, no rebasing/force-push/release, and no global configuration change. An upstream push denial may be recorded as nonblocking when the original master permits a verified local handoff.

6. Progress and safe resumption across session limits
Use only the existing external QUEUE_STATE.md for the live operational phase; tracked goal-XX.md files summarize accepted work and evidence. Before long commands record their exact owner, command/arguments, relevant source identity, deadlines, expected evidence and next recovery action. On completion capture exit/timeout, first failure, stdout/stderr byte-drain status, cleanup/PID identity and any new checkpoint SHA/push. Preserve failed and passing receipts and immutable executed inputs.

Give concise factual progress updates at meaningful checkpoints: current Goal/checkpoint, actual verified pass/failure, new commits and push status, next action and exact blocker if any. If execution pauses because a Codex session ends, say Paused/resumable and preserve the next runnable action; never claim a detached background continuation is still running when it is not.

If interrupted again, re-read the same ledger/master and the newest BLOCKED_RESUME, reconcile actual Git/files/process state under master Section 12, and continue without creating duplicate branch, baseline, test job or queue. Honor the finite recovery budget already consumed; an interruption does not reset its count.

7. Genuine stopping conditions and final report
Continue while an active Codex execution session exists until either:

All 17 Goals have passed their original mandatory implementation, review and verification gates and have actual committed cumulative final anchors; or
A genuine unresolved mandatory blocker, permission/agent/capability or concurrent-work conflict, or exhausted finite repair budget prevents safe continuation.
On a genuine stop, retain all source, evidence and predecessor anchors; do not silently skip Goal04 or create Goal05. Record the exact unmet criterion and smallest essential next authorization. On verified completion of Goal17, create the original master's single consolidated final report for Goals01–17, with real branch/base/baseline/checkpoint/final SHAs, review/test/native/Clippy results, measured outcomes, push states, current cumulative tip, ledger and evidence paths, and honest limitations. Never merge or release automatically.

BEGIN EXECUTION NOW: Read and reconcile QUEUE_STATE.md, the original queue plan, the latest Goal04 M2-A g04-recovery-20261026/m2a-live-r09/BLOCKED_RESUME.md, parent audit and actual controller receipt. Verify the four preserved uncommitted files, current process/Git state, and accepted M1-B ceff1f17…. Record the two-round narrow M2-A renewal. Dispatch a focused Planner packet to diagnose and safely resolve the startup DAP job/admission barrier and compiler-origin diagnostic evidence without global/cache leaks; have the sole Implementer make the minimal permitted correction, perform bounded actual-live acceptance, obtain review, commit/push M2-A only on success, then execute M2-B and all remaining Goals in sequence.